import { describe, expect, test } from "bun:test";

import {
  MINIMUM_FIELD_SAMPLES,
  WEB_PERFORMANCE_METRICS,
  baselineQuery,
  sampleCountQuery,
} from "./web-performance-baseline";

describe("web performance baseline", () => {
  test("uses a reproducible p75 query and published Core Web Vitals thresholds", () => {
    expect(baselineQuery("oore_web_lcp_seconds", "28d")).toBe(
      "histogram_quantile(0.75, sum by (le, channel, persona) (rate(oore_web_lcp_seconds_bucket[28d])))",
    );
    expect(() =>
      baselineQuery("oore_web_lcp_seconds", "28d) or vector(1)"),
    ).toThrow();
    expect(sampleCountQuery("oore_web_lcp_seconds", "28d")).toBe(
      "sum by (channel, persona) (increase(oore_web_lcp_seconds_count[28d]))",
    );
    expect(MINIMUM_FIELD_SAMPLES).toBe(200);
    expect(
      Object.fromEntries(
        WEB_PERFORMANCE_METRICS.map(({ name, threshold }) => [name, threshold]),
      ),
    ).toMatchObject({ lcp: 2.5, inp: 0.2, cls: 0.1, ttfb: 0.8 });
  });
});
