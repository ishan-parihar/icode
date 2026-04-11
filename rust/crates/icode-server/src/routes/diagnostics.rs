use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct CrashReportResponse {
    pub has_crash_report: bool,
    pub report: Option<telemetry::CrashReport>,
}

#[expect(clippy::unused_async)]
pub async fn get_crash_report() -> Json<CrashReportResponse> {
    match telemetry::CrashReport::load_latest() {
        Ok(Some(report)) => Json(CrashReportResponse {
            has_crash_report: true,
            report: Some(report),
        }),
        _ => Json(CrashReportResponse {
            has_crash_report: false,
            report: None,
        }),
    }
}

#[derive(Serialize)]
pub struct DiagnosticResponse {
    pub snapshot: telemetry::DiagnosticSnapshot,
}

#[expect(clippy::unused_async)]
pub async fn get_diagnostics() -> Json<DiagnosticResponse> {
    let snapshot = telemetry::DiagnosticSnapshot::capture();
    Json(DiagnosticResponse { snapshot })
}
