#!/usr/bin/env python3
"""Benchmark regression gate for .larp.

Compares the bench example output (bench-results.txt) against
docs/benchmark-baseline.txt and fails on regressions. Lower is better
for every metric except `picked` (records served within the token
budget), which must not drop.

Thresholds (deliberately loose where runners are noisy):
- byte/token counts: +10% over baseline
- percentage points (larp_vs_*_pct): +2.0 points
- latencies (*_ms): 3x baseline (shared runners vary a lot)
- records: must match exactly
- picked: must not decrease

Usage: python3 .github/scripts/check-bench.py <results> <baseline>
"""

import re
import sys

SIZE_KEYS = {"larp_bytes", "md_bytes", "jsonl_bytes", "used"}
PCT_KEYS = {"larp_vs_md_pct", "larp_vs_jsonl_pct"}
MS_KEYS = {"bulk_write_ms", "search_avg_ms", "recall_avg_ms", "single_append_ms"}


def parse(path):
    values = {}
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            key, _, rest = line.partition(":")
            key = key.strip()
            rest = rest.strip()
            if key == "recall_2000_tokens":
                for part in rest.split():
                    k, _, v = part.partition("=")
                    values[k] = float(v)
            else:
                values[key] = float(rest)
    return values


def main():
    if len(sys.argv) != 3:
        print("usage: check-bench.py <results> <baseline>", file=sys.stderr)
        return 2
    current = parse(sys.argv[1])
    baseline = parse(sys.argv[2])
    regressions = []
    for key, base in baseline.items():
        if key not in current:
            regressions.append(f"{key}: missing from current results")
            continue
        cur = current[key]
        if key == "records":
            if cur != base:
                regressions.append(f"records: {cur} != baseline {base} (update baseline)")
        elif key == "picked":
            if cur < base:
                regressions.append(f"picked: {cur} < baseline {base} (recall serves fewer)")
        elif key in PCT_KEYS:
            if cur > base + 2.0:
                regressions.append(f"{key}: {cur} > baseline {base} + 2.0 points")
        elif key in MS_KEYS:
            if cur > base * 3:
                regressions.append(f"{key}: {cur} > baseline {base} x3 (latency)")
        else:
            if cur > base * 1.10:
                regressions.append(f"{key}: {cur} > baseline {base} +10%")
    if regressions:
        print("Benchmark regressions found:")
        for r in regressions:
            print(f"  - {r}")
        return 1
    print("Benchmark gate passed (no regressions vs baseline).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
