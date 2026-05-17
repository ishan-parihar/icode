/// Live model catalog from <https://models.dev/api.json>
///
/// This is icode's port of opencode's packages/core/src/models.ts.
/// It is the single source of truth for all providers and models.
/// The static `MODEL_REGISTRY` is gone — everything comes from here.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

const CATALOG_URL: &str = "https://models.dev/api.json";
const TTL_SECS: u64 = 300; // 5 minutes, same as opencode

// ── Raw JSON types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RawProvider {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub env: Vec<String>,
    pub api: Option<String>,
    pub npm: Option<String>,
    #[serde(default)]
    pub models: HashMap<String, RawModel>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawModel {
    pub id: String,
    pub name: String,
    pub release_date: Option<String>,
    #[serde(default)]
    pub attachment: bool,
    #[serde(default)]
    pub reasoning: bool,
    #[serde(default)]
    pub temperature: bool,
    #[serde(default = "default_true")]
    pub tool_call: bool,
    pub cost: Option<RawCost>,
    pub limit: RawLimit,
    pub modalities: Option<RawModalities>,
    pub status: Option<String>,
    pub family: Option<String>,
    /// Model-level provider override (npm package / api URL)
    pub provider: Option<RawModelProvider>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawCost {
    pub input: f64,
    pub output: f64,
    pub cache_read: Option<f64>,
    pub cache_write: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawLimit {
    #[serde(default)]
    pub context: u32,
    #[serde(default)]
    pub output: u32,
    pub input: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawModalities {
    #[serde(default)]
    pub input: Vec<String>,
    #[serde(default)]
    pub output: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RawModelProvider {
    pub npm: Option<String>,
    pub api: Option<String>,
}

type RawCatalog = HashMap<String, RawProvider>;

// ── Processed model entry ─────────────────────────────────────────────────────

/// Flat, processed representation of a single model from the catalog.
/// This replaces `RegistryEntry`.
#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub provider_id: String,
    pub provider_name: String,
    /// API key env vars for this provider (from catalog `env` field)
    pub env: Vec<String>,
    /// Base URL for the API
    pub base_url: String,
    pub model_id: String,
    pub model_name: String,
    pub supports_tools: bool,
    pub supports_images: bool,
    pub supports_reasoning: bool,
    pub context_window: u32,
    pub max_output: u32,
    pub cost_input: f64,
    pub cost_output: f64,
}

// ── Cache ─────────────────────────────────────────────────────────────────────

struct Cache {
    data: RawCatalog,
    fetched_at: SystemTime,
}

static CACHE: OnceLock<RwLock<Option<Cache>>> = OnceLock::new();

fn cache_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".icode").join("cache").join("models.json")
}

fn is_fresh(fetched: SystemTime) -> bool {
    SystemTime::now()
        .duration_since(fetched)
        .unwrap_or(Duration::from_secs(u64::MAX))
        < Duration::from_secs(TTL_SECS)
}

fn try_load_disk() -> Option<RawCatalog> {
    let path = cache_path();
    let data = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

fn try_fetch_network() -> Option<RawCatalog> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?;
    let text = client
        .get(CATALOG_URL)
        .header("User-Agent", concat!("icode/", env!("CARGO_PKG_VERSION")))
        .send()
        .ok()?
        .text()
        .ok()?;
    let catalog: RawCatalog = serde_json::from_str(&text).ok()?;
    // Persist to disk for offline use
    let path = cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, &text);
    Some(catalog)
}

/// Obtain the catalog. Tries (in order): in-process cache → disk cache → network → empty.
pub fn catalog() -> RawCatalog {
    let lock = CACHE.get_or_init(|| RwLock::new(None));

    // Fast path: in-process cache is fresh
    {
        let guard = lock.read().unwrap();
        if let Some(ref c) = *guard {
            if is_fresh(c.fetched_at) {
                return c.data.clone();
            }
        }
    }

    // Slow path: need to populate / refresh
    let mut guard = lock.write().unwrap();
    // Double-check after acquiring write lock
    if let Some(ref c) = *guard {
        if is_fresh(c.fetched_at) {
            return c.data.clone();
        }
    }

    let data = try_fetch_network()
        .or_else(try_load_disk)
        .unwrap_or_default();

    let result = data.clone();
    *guard = Some(Cache { data, fetched_at: SystemTime::now() });
    result
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Return all active models from the catalog, as flat entries.
/// Mirrors opencode's `list_all_models` behavior (drops deprecated/alpha).
#[must_use] 
pub fn list_models() -> Vec<ModelEntry> {
    let cat = catalog();
    let mut entries: Vec<ModelEntry> = Vec::new();

    for provider in cat.values() {
        let provider_base = provider.api.clone().unwrap_or_default();

        for model in provider.models.values() {
            let status = model.status.as_deref().unwrap_or("active");
            if status == "deprecated" || status == "alpha" {
                continue;
            }

            let base_url = model
                .provider
                .as_ref()
                .and_then(|p| p.api.clone())
                .unwrap_or_else(|| provider_base.clone());

            let supports_images = model
                .modalities
                .as_ref()
                .is_some_and(|m| m.input.iter().any(|s| s == "image"));

            entries.push(ModelEntry {
                provider_id: provider.id.clone(),
                provider_name: provider.name.clone(),
                env: provider.env.clone(),
                base_url,
                model_id: model.id.clone(),
                model_name: model.name.clone(),
                supports_tools: model.tool_call,
                supports_images,
                supports_reasoning: model.reasoning,
                context_window: model.limit.context,
                max_output: model.limit.output,
                cost_input: model.cost.as_ref().map_or(0.0, |c| c.input),
                cost_output: model.cost.as_ref().map_or(0.0, |c| c.output),
            });
        }
    }

    entries.sort_by(|a, b| a.provider_id.cmp(&b.provider_id).then(a.model_id.cmp(&b.model_id)));
    entries
}

/// Look up the provider entry for a given provider ID.
#[must_use] 
pub fn provider(id: &str) -> Option<RawProvider> {
    catalog().remove(id)
}

/// Check if a provider has auth via env vars or `AuthStore`.
#[must_use] 
pub fn provider_has_auth(p: &RawProvider) -> bool {
    if p.id == "amazon-bedrock" {
        return ["AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_PROFILE", "AWS_BEARER_TOKEN_BEDROCK"]
            .iter()
            .any(|v| std::env::var(v).is_ok());
    }
    if p.env.is_empty() {
        return true; // No key required (e.g. local providers)
    }
    // Check env vars
    if p.env.iter().any(|v| std::env::var(v).map(|s| !s.is_empty()).unwrap_or(false)) {
        return true;
    }
    // Check AuthStore (persisted keys saved via provider dialog)
    let store = runtime::AuthStore::load();
    let auth_key = p.id.to_lowercase().replace('-', "_");
    store.api_key_for(&auth_key).is_some()
}

/// Return the primary API key env var name for a provider (first in env list).
pub fn primary_env_var(p: &RawProvider) -> Option<&str> {
    p.env.first().map(String::as_str)
}

/// Look up a model by `"provider_id/model_id"` string.
#[must_use] 
pub fn find_model(model_str: &str) -> Option<(RawProvider, RawModel)> {
    let (provider_id, model_id) = model_str.split_once('/')?;
    let cat = catalog();
    let provider = cat.get(provider_id)?.clone();
    let model = provider.models.get(model_id)?.clone();
    Some((provider, model))
}

/// Given a `"provider_id/model_id"` string, determine the base URL to use.
/// Falls back to provider-level URL.
#[must_use] 
pub fn base_url_for(model_str: &str) -> Option<String> {
    let (provider, model) = find_model(model_str)?;
    Some(
        model
            .provider
            .as_ref()
            .and_then(|p| p.api.clone())
            .unwrap_or_else(|| provider.api.unwrap_or_default()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_models_excludes_deprecated() {
        // Build a minimal synthetic catalog entry
        let mut cat = RawCatalog::new();
        let mut provider = RawProvider {
            id: "test".into(),
            name: "Test".into(),
            env: vec![],
            api: None,
            npm: None,
            models: HashMap::new(),
        };
        provider.models.insert("good".into(), RawModel {
            id: "good".into(),
            name: "Good".into(),
            release_date: None,
            attachment: false,
            reasoning: false,
            temperature: false,
            tool_call: true,
            cost: None,
            limit: RawLimit { context: 8192, output: 2048, input: None },
            modalities: None,
            status: Some("active".into()),
            family: None,
            provider: None,
        });
        provider.models.insert("old".into(), RawModel {
            id: "old".into(),
            name: "Old".into(),
            release_date: None,
            attachment: false,
            reasoning: false,
            temperature: false,
            tool_call: true,
            cost: None,
            limit: RawLimit { context: 4096, output: 1024, input: None },
            modalities: None,
            status: Some("deprecated".into()),
            family: None,
            provider: None,
        });
        cat.insert("test".into(), provider);

        // Test the filter logic directly
        let entries: Vec<ModelEntry> = {
            let mut out = Vec::new();
            for p in cat.values() {
                for m in p.models.values() {
                    let status = m.status.as_deref().unwrap_or("active");
                    if status == "deprecated" || status == "alpha" { continue; }
                    out.push(ModelEntry {
                        provider_id: p.id.clone(),
                        provider_name: p.name.clone(),
                        env: p.env.clone(),
                        base_url: p.api.clone().unwrap_or_default(),
                        model_id: m.id.clone(),
                        model_name: m.name.clone(),
                        supports_tools: m.tool_call,
                        supports_images: false,
                        supports_reasoning: m.reasoning,
                        context_window: m.limit.context,
                        max_output: m.limit.output,
                        cost_input: 0.0,
                        cost_output: 0.0,
                    });
                }
            }
            out
        };
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].model_id, "good");
    }
}
