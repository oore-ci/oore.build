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
    Router,
    extract::{MatchedPath, Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::get,
};
use metrics::{counter, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

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
    let builder = PrometheusBuilder::new();
    builder
        .install_recorder()
        .expect("failed to install Prometheus metrics recorder")
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
        .unwrap_or_else(|| request.uri().path().to_string());

    let start = Instant::now();
    let response = next.run(request).await;
    let elapsed = start.elapsed().as_secs_f64();

    let status = response.status().as_u16().to_string();

    let labels = [("method", method), ("path", path), ("status", status)];

    counter!("http_requests_total", &labels).increment(1);
    histogram!("http_request_duration_seconds", &labels).record(elapsed);

    response
}
