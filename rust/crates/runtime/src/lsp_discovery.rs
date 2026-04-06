use std::path::Path;
#[derive(Debug, Clone)]
pub struct LspServerDef { pub language: &'static str, pub command: &'static str, pub args: &'static [&'static str], pub extensions: &'static [&'static str], pub markers: &'static [&'static str], pub github_repo: &'static str, pub binary_name: &'static str }
#[derive(Debug, Clone)]
pub struct DiscoveryEntry { pub server: LspServerDef, pub installed: bool }
#[derive(Debug, Clone)]
pub struct DiscoveryResult { pub path: String, pub servers: Vec<DiscoveryEntry> }
pub fn auto_discover(path: &Path) -> DiscoveryResult { let ps = path.to_string_lossy(); DiscoveryResult { path: ps.to_string(), servers: discover(&ps) } }
fn discover(path: &str) -> Vec<DiscoveryEntry> { let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_string(); let mut r = Vec::new(); for s in builtin() { let me = s.extensions.iter().any(|e| *e == ext); let mm = !s.markers.is_empty() && root(path).map_or(false, |ro| s.markers.iter().any(|m| Path::new(&ro).join(m).exists())); if me || mm { r.push(DiscoveryEntry { server: s.clone(), installed: installed(s.command) }); } } r }
fn root(path: &str) -> Option<String> { let mut c = Path::new(path).canonicalize().ok()?; let ms = [".git","Cargo.toml","package.json","go.mod","pyproject.toml","requirements.txt","setup.py","pom.xml","build.gradle","CMakeLists.txt","Makefile",".svn"]; loop { for m in &ms { if c.join(m).exists() { return Some(c.to_string_lossy().to_string()); } } if !c.pop() { break; } } None }
pub fn installed(cmd: &str) -> bool { if let Ok(p) = std::env::var("PATH") { for d in p.split(':') { let c = Path::new(d).join(cmd); if c.exists() && c.is_file() { #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; if let Ok(m) = c.metadata() { if m.permissions().mode() & 0o111 != 0 { return true; } } } #[cfg(not(unix))] return true; } } } false }
pub fn download_server(s: &LspServerDef) -> Result<String, String> { if installed(s.command) { return Ok(s.command.to_owned()); } Err(format!("{} is not installed. Download from https://github.com/{}/releases and ensure it is in PATH.", s.command, s.github_repo)) }
fn builtin() -> &'static [LspServerDef] { &[
    LspServerDef { language: "rust", command: "rust-analyzer", args: &[], extensions: &["rs"], markers: &["Cargo.toml"], github_repo: "rust-lang/rust-analyzer", binary_name: "rust-analyzer" },
    LspServerDef { language: "typescript", command: "typescript-language-server", args: &["--stdio"], extensions: &["ts","tsx"], markers: &["package.json","tsconfig.json"], github_repo: "typescript-language-server/typescript-language-server", binary_name: "typescript-language-server" },
    LspServerDef { language: "javascript", command: "typescript-language-server", args: &["--stdio"], extensions: &["js","jsx","mjs","cjs"], markers: &["package.json"], github_repo: "typescript-language-server/typescript-language-server", binary_name: "typescript-language-server" },
    LspServerDef { language: "python", command: "pyright-langserver", args: &["--stdio"], extensions: &["py"], markers: &["pyproject.toml","requirements.txt","setup.py"], github_repo: "microsoft/pyright", binary_name: "pyright-langserver" },
    LspServerDef { language: "go", command: "gopls", args: &[], extensions: &["go"], markers: &["go.mod"], github_repo: "golang/tools/gopls", binary_name: "gopls" },
    LspServerDef { language: "java", command: "jdtls", args: &[], extensions: &["java"], markers: &["pom.xml","build.gradle","build.gradle.kts"], github_repo: "eclipse-jdtls/eclipse.jdt.ls", binary_name: "jdtls" },
    LspServerDef { language: "c", command: "clangd", args: &[], extensions: &["c","h"], markers: &["CMakeLists.txt","compile_commands.json","Makefile"], github_repo: "clangd/clangd", binary_name: "clangd" },
    LspServerDef { language: "cpp", command: "clangd", args: &[], extensions: &["cpp","hpp","cc","cxx","hxx"], markers: &["CMakeLists.txt","compile_commands.json","Makefile"], github_repo: "clangd/clangd", binary_name: "clangd" },
    LspServerDef { language: "ruby", command: "ruby-lsp", args: &["stdio"], extensions: &["rb"], markers: &["Gemfile"], github_repo: "Shopify/ruby-lsp", binary_name: "ruby-lsp" },
    LspServerDef { language: "lua", command: "lua-language-server", args: &[], extensions: &["lua"], markers: &[], github_repo: "LuaLS/lua-language-server", binary_name: "lua-language-server" },
    LspServerDef { language: "php", command: "intelephense", args: &["--stdio"], extensions: &["php"], markers: &["composer.json"], github_repo: "bmewburn/vscode-intelephense", binary_name: "intelephense" },
    LspServerDef { language: "zig", command: "zls", args: &[], extensions: &["zig"], markers: &["build.zig"], github_repo: "zigtools/zls", binary_name: "zls" },
]}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn rust_ext() { let r = auto_discover(Path::new("/p/main.rs")); let s = r.servers.iter().find(|e| e.server.language == "rust").unwrap(); assert_eq!(s.server.command, "rust-analyzer"); }
    #[test] fn ts_ext() { let r = auto_discover(Path::new("/p/index.ts")); assert!(r.servers.iter().any(|e| e.server.language == "typescript")); }
    #[test] fn py_ext() { let r = auto_discover(Path::new("/p/main.py")); assert!(r.servers.iter().any(|e| e.server.language == "python")); }
    #[test] fn go_ext() { let r = auto_discover(Path::new("/p/main.go")); assert!(r.servers.iter().any(|e| e.server.language == "go")); }
    #[test] fn rb_ext() { let r = auto_discover(Path::new("/p/app.rb")); assert!(r.servers.iter().any(|e| e.server.language == "ruby")); }
    #[test] fn unknown_empty() { assert!(auto_discover(Path::new("/p/d.csv")).servers.is_empty()); }
    #[test] fn not_installed() { assert!(!installed("nonexistent-xyz")); }
    #[test] fn download_err() { let r = download_server(&builtin()[0]); assert!(r.is_err()); assert!(r.unwrap_err().contains("github.com")); }
    #[test] fn all_fields() { let s = builtin(); let v: Vec<_> = s.iter().filter(|x| !x.language.is_empty() && !x.command.is_empty() && !x.extensions.is_empty()).collect(); assert_eq!(v.len(), s.len()); }
    #[test] fn count12() { assert_eq!(builtin().len(), 12); }
    #[test] fn tsx() { assert!(auto_discover(Path::new("/p/A.tsx")).servers.iter().any(|e| e.server.language == "typescript")); }
    #[test] fn hpp() { assert!(auto_discover(Path::new("/p/u.hpp")).servers.iter().any(|e| e.server.language == "cpp")); }
    #[test] fn h_ext() { assert!(auto_discover(Path::new("/p/u.h")).servers.iter().any(|e| e.server.language == "c")); }
    #[test] fn mjs() { assert!(auto_discover(Path::new("/p/u.mjs")).servers.iter().any(|e| e.server.language == "javascript")); }
    #[test] fn cxx() { assert!(auto_discover(Path::new("/p/m.cxx")).servers.iter().any(|e| e.server.language == "cpp")); }
    #[test] fn cc() { assert!(auto_discover(Path::new("/p/m.cc")).servers.iter().any(|e| e.server.language == "cpp")); }
    #[test] fn js_args() { assert_eq!(builtin().iter().find(|x| x.language == "javascript").unwrap().args, &["--stdio"]); }
    #[test] fn ts_args() { assert_eq!(builtin().iter().find(|x| x.language == "typescript").unwrap().args, &["--stdio"]); }
    #[test] fn py_args() { assert_eq!(builtin().iter().find(|x| x.language == "python").unwrap().args, &["--stdio"]); }
    #[test] fn ruby_args() { assert_eq!(builtin().iter().find(|x| x.language == "ruby").unwrap().args, &["stdio"]); }
    #[test] fn rust_no_args() { assert!(builtin().iter().find(|x| x.language == "rust").unwrap().args.is_empty()); }
    #[test] fn go_no_args() { assert!(builtin().iter().find(|x| x.language == "go").unwrap().args.is_empty()); }
    #[test] fn rust_repo() { assert_eq!(builtin().iter().find(|x| x.language == "rust").unwrap().github_repo, "rust-lang/rust-analyzer"); }
    #[test] fn py_repo() { assert_eq!(builtin().iter().find(|x| x.language == "python").unwrap().github_repo, "microsoft/pyright"); }
    #[test] fn gopls_repo() { assert_eq!(builtin().iter().find(|x| x.language == "go").unwrap().github_repo, "golang/tools/gopls"); }
    #[test] fn clangd_repo() { assert_eq!(builtin().iter().find(|x| x.language == "c").unwrap().github_repo, "clangd/clangd"); }
}
