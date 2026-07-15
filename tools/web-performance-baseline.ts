const WINDOW_PATTERN = /^[1-9]\d*(?:ms|s|m|h|d|w|y)$/;
export const MINIMUM_FIELD_SAMPLES = 200;

export const WEB_PERFORMANCE_METRICS = [
  { name: "lcp", prometheus: "oore_web_lcp_seconds", threshold: 2.5 },
  { name: "inp", prometheus: "oore_web_inp_seconds", threshold: 0.2 },
  { name: "cls", prometheus: "oore_web_cls_ratio", threshold: 0.1 },
  { name: "ttfb", prometheus: "oore_web_ttfb_seconds", threshold: 0.8 },
  {
    name: "dom_content_loaded",
    prometheus: "oore_web_dom_content_loaded_seconds",
    threshold: null,
  },
  { name: "load", prometheus: "oore_web_load_seconds", threshold: null },
] as const;

export function baselineQuery(prometheus: string, window: string): string {
  if (!WINDOW_PATTERN.test(window))
    throw new Error(`invalid WINDOW: ${window}`);
  return `histogram_quantile(0.75, sum by (le, channel, persona) (rate(${prometheus}_bucket[${window}])))`;
}

export function sampleCountQuery(prometheus: string, window: string): string {
  if (!WINDOW_PATTERN.test(window))
    throw new Error(`invalid WINDOW: ${window}`);
  return `sum by (channel, persona) (increase(${prometheus}_count[${window}]))`;
}

function resultValue(result: { value?: [number, string] }): number | null {
  const value = Number(result.value?.[1]);
  return Number.isFinite(value) ? value : null;
}

async function main() {
  const baseUrl = process.env.PROMETHEUS_URL;
  if (!baseUrl) throw new Error("PROMETHEUS_URL is required");
  const window = process.env.WINDOW || "28d";
  const headers = process.env.PROMETHEUS_BEARER_TOKEN
    ? { Authorization: `Bearer ${process.env.PROMETHEUS_BEARER_TOKEN}` }
    : undefined;

  async function query(prometheusQuery: string) {
    const url = new URL("/api/v1/query", baseUrl);
    url.searchParams.set("query", prometheusQuery);
    const response = await fetch(url, { headers });
    if (!response.ok) throw new Error(`Prometheus returned ${response.status}`);
    const body = (await response.json()) as {
      status: string;
      data?: {
        result?: Array<{
          metric?: Record<string, string>;
          value?: [number, string];
        }>;
      };
    };
    if (body.status !== "success")
      throw new Error("invalid Prometheus response");
    return body.data?.result ?? [];
  }

  const metrics = [];
  for (const metric of WEB_PERFORMANCE_METRICS) {
    const [percentiles, counts] = await Promise.all([
      query(baselineQuery(metric.prometheus, window)),
      query(sampleCountQuery(metric.prometheus, window)),
    ]);
    const samples = new Map(
      counts.map((result) => [
        `${result.metric?.channel}\0${result.metric?.persona}`,
        resultValue(result) ?? 0,
      ]),
    );
    metrics.push({
      name: metric.name,
      threshold: metric.threshold,
      series: percentiles.map((result) => {
        const value = resultValue(result);
        const sampleCount =
          samples.get(`${result.metric?.channel}\0${result.metric?.persona}`) ??
          0;
        const sufficientData = sampleCount >= MINIMUM_FIELD_SAMPLES;
        return {
          channel: result.metric?.channel ?? "unknown",
          persona: result.metric?.persona ?? "unknown",
          samples: sampleCount,
          p75: value,
          sufficient_data: sufficientData,
          pass:
            metric.threshold == null || value == null || !sufficientData
              ? null
              : value <= metric.threshold,
        };
      }),
    });
  }

  console.log(
    JSON.stringify(
      {
        percentile: 75,
        minimum_samples: MINIMUM_FIELD_SAMPLES,
        window,
        generated_at: new Date().toISOString(),
        metrics,
      },
      null,
      2,
    ),
  );
}

if (import.meta.main) {
  await main();
}
