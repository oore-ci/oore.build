//! Observability setup: OpenTelemetry tracing and Prometheus metrics.
//!
//! - **OTel tracing** is opt-in via `OTEL_EXPORTER_OTLP_ENDPOINT`. When the env
//!   var is unset the daemon runs with only the `tracing_subscriber::fmt` layer,
//!   keeping it zero-cost.
//! - **Prometheus `/metrics`** is always available. An Axum middleware records
//!   `http_requests_total` (counter) and `http_request_duration_seconds`
//!   (histogram) per method/path/status.

use std::time::Instant;

use axum::{
    Json, Router,
    extract::{MatchedPath, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    routing::get,
};
use metrics::{counter, histogram};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use oore_contract::ApiError;
use serde::Deserialize;

use crate::extractors::AuthUser;
use crate::util::api_err;

// ── OpenTelemetry tracing setup ─────────────────────────────────

/// Initialise the `tracing_subscriber` registry.
///
/// When `OTEL_EXPORTER_OTLP_ENDPOINT` is set, a `tracing_opentelemetry`
/// layer is added that exports spans via OTLP/gRPC. Otherwise only the
/// human-readable `fmt` layer is installed.
pub fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let fmt_layer = tracing_subscriber::fmt::layer();
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    if std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").is_ok() {
        match build_otel_layer() {
            Ok(otel_layer) => {
                tracing_subscriber::registry()
                    .with(filter)
                    .with(fmt_layer)
                    .with(otel_layer)
                    .init();
                tracing::info!("OpenTelemetry tracing layer installed");
                return;
            }
            Err(e) => {
                // Fall back to fmt-only so the daemon can still start.
                eprintln!("WARNING: failed to initialise OpenTelemetry layer: {e}");
            }
        }
    }

    // No OTel endpoint or OTel init failed — fmt-only.
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();
}

/// Build the OpenTelemetry tracing layer backed by an OTLP/gRPC exporter.
///
/// Returns a layer that is generic over `S` so it can compose with any
/// subscriber stack built from `tracing_subscriber::Registry`.
fn build_otel_layer<S>() -> Result<
    tracing_opentelemetry::OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>,
    Box<dyn std::error::Error>,
>
where
    S: tracing::Subscriber + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
{
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::SpanExporter;
    use opentelemetry_sdk::Resource;
    use opentelemetry_sdk::trace::SdkTracerProvider;

    let exporter = SpanExporter::builder().with_tonic().build()?;

    let resource = Resource::builder().with_service_name("oored").build();

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource)
        .build();

    let tracer = provider.tracer("oored");

    // Register the provider globally so shutdown can flush spans.
    opentelemetry::global::set_tracer_provider(provider);

    Ok(tracing_opentelemetry::layer().with_tracer(tracer))
}

/// Best-effort OTel shutdown — flushes any buffered spans.
///
/// The global tracer provider is replaced with a no-op; this triggers
/// the `SdkTracerProvider::Drop` which flushes the batch exporter.
pub fn shutdown_tracing() {
    // Replacing the global provider with a no-op drops the previous
    // provider, which flushes buffered spans in the batch exporter.
    let _previous = opentelemetry::global::set_tracer_provider(
        opentelemetry::trace::noop::NoopTracerProvider::new(),
    );
}

// ── Prometheus metrics ──────────────────────────────────────────

/// Install the `metrics` recorder backed by Prometheus and return the
/// handle used to render the text exposition format.
pub fn init_metrics() -> PrometheusHandle {
    metrics_builder()
        .install_recorder()
        .expect("failed to install Prometheus metrics recorder")
}

pub(crate) fn metrics_builder() -> PrometheusBuilder {
    const PAGE_SECONDS: &[f64] = &[0.1, 0.2, 0.4, 0.8, 1.0, 1.8, 2.5, 4.0, 8.0, 15.0, 60.0];

    PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("oore_web_lcp_seconds".to_string()),
            &[0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 6.0, 10.0, 20.0, 60.0],
        )
        .expect("valid LCP buckets")
        .set_buckets_for_metric(
            Matcher::Full("oore_web_inp_seconds".to_string()),
            &[0.05, 0.1, 0.2, 0.3, 0.5, 0.8, 1.0, 2.0, 5.0, 10.0],
        )
        .expect("valid INP buckets")
        .set_buckets_for_metric(
            Matcher::Full("oore_web_cls_ratio".to_string()),
            &[0.01, 0.05, 0.1, 0.15, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0],
        )
        .expect("valid CLS buckets")
        .set_buckets_for_metric(
            Matcher::Full("oore_web_ttfb_seconds".to_string()),
            PAGE_SECONDS,
        )
        .expect("valid TTFB buckets")
        .set_buckets_for_metric(
            Matcher::Full("oore_web_dom_content_loaded_seconds".to_string()),
            PAGE_SECONDS,
        )
        .expect("valid DOM content loaded buckets")
        .set_buckets_for_metric(
            Matcher::Full("oore_web_load_seconds".to_string()),
            PAGE_SECONDS,
        )
        .expect("valid load buckets")
}

const MAX_WEB_VITAL_OBSERVATIONS: usize = 8;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WebReleaseChannel {
    Dev,
    Alpha,
    Beta,
    Stable,
}

impl WebReleaseChannel {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Alpha => "alpha",
            Self::Beta => "beta",
            Self::Stable => "stable",
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WebPersona {
    OperatorShell,
    MobileShell,
    Admin,
    OperatorBuildDetail,
    QaShell,
    QaInstall,
}

impl WebPersona {
    const fn as_str(self) -> &'static str {
        match self {
            Self::OperatorShell => "operator_shell",
            Self::MobileShell => "mobile_shell",
            Self::Admin => "admin",
            Self::OperatorBuildDetail => "operator_build_detail",
            Self::QaShell => "qa_shell",
            Self::QaInstall => "qa_install",
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WebPerformanceMetric {
    Lcp,
    Inp,
    Cls,
    Ttfb,
    DomContentLoaded,
    Load,
    RenderError,
    UnhandledRejection,
}

#[derive(Deserialize)]
struct WebPerformanceObservation {
    metric: WebPerformanceMetric,
    value: f64,
}

impl WebPerformanceObservation {
    fn validate(&self) -> bool {
        self.value.is_finite()
            && self.value >= 0.0
            && match self.metric {
                WebPerformanceMetric::Cls => self.value <= 10.0,
                WebPerformanceMetric::RenderError | WebPerformanceMetric::UnhandledRejection => {
                    self.value == 1.0
                }
                _ => self.value <= 120_000.0,
            }
    }

    fn record(&self, channel: WebReleaseChannel, persona: WebPersona) {
        let labels = [("channel", channel.as_str()), ("persona", persona.as_str())];
        match self.metric {
            WebPerformanceMetric::Lcp => {
                histogram!("oore_web_lcp_seconds", &labels).record(self.value / 1000.0)
            }
            WebPerformanceMetric::Inp => {
                histogram!("oore_web_inp_seconds", &labels).record(self.value / 1000.0)
            }
            WebPerformanceMetric::Cls => {
                histogram!("oore_web_cls_ratio", &labels).record(self.value)
            }
            WebPerformanceMetric::Ttfb => {
                histogram!("oore_web_ttfb_seconds", &labels).record(self.value / 1000.0)
            }
            WebPerformanceMetric::DomContentLoaded => {
                histogram!("oore_web_dom_content_loaded_seconds", &labels)
                    .record(self.value / 1000.0)
            }
            WebPerformanceMetric::Load => {
                histogram!("oore_web_load_seconds", &labels).record(self.value / 1000.0)
            }
            WebPerformanceMetric::RenderError => {
                counter!("oore_web_render_errors_total", &labels).increment(1)
            }
            WebPerformanceMetric::UnhandledRejection => {
                counter!("oore_web_unhandled_rejections_total", &labels).increment(1)
            }
        }
    }
}

#[derive(Deserialize)]
pub struct WebPerformanceRequest {
    channel: WebReleaseChannel,
    persona: WebPersona,
    observations: Vec<WebPerformanceObservation>,
}

/// Records authenticated, anonymous browser performance and reliability metrics.
///
/// The fixed enums deliberately exclude URLs, user IDs, device IDs, and arbitrary labels.
pub async fn record_web_performance(
    _auth: AuthUser,
    Json(request): Json<WebPerformanceRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if request.observations.is_empty()
        || request.observations.len() > MAX_WEB_VITAL_OBSERVATIONS
        || request
            .observations
            .iter()
            .any(|observation| !observation.validate())
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_web_performance_observation",
            "Web performance observations must contain 1-8 bounded values",
        ));
    }

    for observation in &request.observations {
        observation.record(request.channel, request.persona);
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Axum handler for `GET /metrics`.
async fn metrics_handler(State(handle): State<PrometheusHandle>) -> impl IntoResponse {
    handle.render()
}

/// Returns a sub-router that serves `GET /metrics` using the provided
/// Prometheus handle.
pub fn metrics_router(handle: PrometheusHandle) -> Router {
    Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(handle)
}

// ── Request metrics middleware ───────────────────────────────────

/// Axum middleware that records per-request metrics:
///
/// - `http_requests_total{method, path, status}` — counter
/// - `http_request_duration_seconds{method, path, status}` — histogram
pub async fn track_http_metrics(request: Request, next: Next) -> Response {
    let method = request.method().to_string();

    // Use the matched route pattern (e.g. "/v1/public/setup-status") when
    // available so we don't explode label cardinality with path parameters.
    let path = request
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "__unmatched__".to_string());

    let start = Instant::now();
    let response = next.run(request).await;
    let elapsed = start.elapsed().as_secs_f64();

    let status = response.status().as_u16().to_string();

    let labels = [("method", method), ("path", path), ("status", status)];

    counter!("http_requests_total", &labels).increment(1);
    histogram!("http_request_duration_seconds", &labels).record(elapsed);

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_performance_contract_rejects_unbounded_or_arbitrary_data() {
        let valid: WebPerformanceRequest = serde_json::from_value(serde_json::json!({
            "channel": "alpha",
            "persona": "qa_install",
            "observations": [{"metric": "lcp", "value": 2499.0}]
        }))
        .expect("valid fixed-cardinality observation");
        assert!(valid.observations[0].validate());

        let arbitrary = serde_json::from_value::<WebPerformanceRequest>(serde_json::json!({
            "channel": "alpha",
            "persona": "/builds/private-id",
            "observations": [{"metric": "lcp", "value": 1.0}]
        }));
        assert!(arbitrary.is_err());

        let unbounded: WebPerformanceRequest = serde_json::from_value(serde_json::json!({
            "channel": "alpha",
            "persona": "qa_install",
            "observations": [{"metric": "lcp", "value": 120001.0}]
        }))
        .expect("shape is valid");
        assert!(!unbounded.observations[0].validate());
    }
}
