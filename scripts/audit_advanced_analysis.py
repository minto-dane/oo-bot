#!/usr/bin/env python3
"""Generate detailed audit analytics (JSON/CSV/optional charts) from oo-bot SQLite audit DB."""

from __future__ import annotations

import argparse
import csv
import json
import math
import os
import sqlite3
from collections import Counter
from pathlib import Path
from typing import Iterable, List, Sequence, Tuple


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="oo-bot audit advanced analytics")
    parser.add_argument(
        "--db",
        default="state/audit/events.sqlite3",
        help="path to audit sqlite database",
    )
    parser.add_argument(
        "--out-dir",
        default="state/security/audit-advanced-analysis",
        help="directory for output artifacts",
    )
    parser.add_argument(
        "--chart",
        action="store_true",
        help="generate PNG charts (requires matplotlib)",
    )
    parser.add_argument(
        "--max-top-readings",
        type=int,
        default=30,
        help="top-N matched readings to keep in outputs",
    )
    return parser.parse_args()


def fetch_counter(
    conn: sqlite3.Connection, query: str, params: Sequence[object] | None = None
) -> Counter:
    rows = conn.execute(query, params or []).fetchall()
    out: Counter = Counter()
    for key, count in rows:
        if key is None:
            continue
        out[str(key)] = int(count)
    return out


def write_counter_csv(path: Path, header: Tuple[str, str], counter: Counter) -> None:
    with path.open("w", encoding="utf-8", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(header)
        for key, count in counter.most_common():
            writer.writerow([key, count])


def percentile(values: List[float], p: float) -> float:
    if not values:
        return 0.0
    if p <= 0:
        return values[0]
    if p >= 100:
        return values[-1]
    idx = (len(values) - 1) * (p / 100.0)
    lo = math.floor(idx)
    hi = math.ceil(idx)
    if lo == hi:
        return values[lo]
    frac = idx - lo
    return values[lo] * (1 - frac) + values[hi] * frac


def build_matched_reading_counter(conn: sqlite3.Connection) -> Counter:
    counter: Counter = Counter()
    rows = conn.execute("SELECT matched_readings_json FROM audit_events").fetchall()
    for (raw,) in rows:
        if not raw:
            continue
        try:
            data = json.loads(raw)
        except json.JSONDecodeError:
            continue
        if not isinstance(data, list):
            continue
        for item in data:
            if isinstance(item, str) and item:
                counter[item] += 1
    return counter


def build_processing_stats(conn: sqlite3.Connection) -> dict:
    rows = conn.execute(
        "SELECT processing_time_ms FROM audit_events WHERE processing_time_ms IS NOT NULL"
    ).fetchall()
    values = sorted(float(r[0]) for r in rows)
    if not values:
        return {
            "count": 0,
            "avg_ms": 0.0,
            "p50_ms": 0.0,
            "p95_ms": 0.0,
            "p99_ms": 0.0,
            "max_ms": 0.0,
        }
    return {
        "count": len(values),
        "avg_ms": sum(values) / len(values),
        "p50_ms": percentile(values, 50),
        "p95_ms": percentile(values, 95),
        "p99_ms": percentile(values, 99),
        "max_ms": values[-1],
    }


def write_timeline_csv(path: Path, rows: Iterable[Tuple[str, int]]) -> None:
    with path.open("w", encoding="utf-8", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(["hour_utc", "events"])
        for hour, count in rows:
            writer.writerow([hour, int(count)])


def maybe_generate_charts(out_dir: Path, series: dict[str, Counter], timeline_rows: list[Tuple[str, int]]):
    try:
        import matplotlib.pyplot as plt  # type: ignore
    except Exception:
        print("[warn] matplotlib not available; skip chart generation")
        return

    def bar_chart(counter: Counter, title: str, file_name: str, top_n: int = 15) -> None:
        items = counter.most_common(top_n)
        if not items:
            return
        labels = [k for k, _ in items]
        values = [v for _, v in items]
        plt.figure(figsize=(12, 6))
        plt.bar(labels, values)
        plt.title(title)
        plt.xticks(rotation=35, ha="right")
        plt.tight_layout()
        plt.savefig(out_dir / file_name, dpi=140)
        plt.close()

    bar_chart(series["event_type"], "Event Type Counts", "event_type_counts.png")
    bar_chart(series["selected_action"], "Response Action Counts", "response_action_counts.png")
    bar_chart(series["suppressed_reason"], "Suppression Reason Counts", "suppression_reason_counts.png")
    bar_chart(series["mode"], "Runtime Mode Counts", "runtime_mode_counts.png")
    bar_chart(series["matched_readings"], "Top Matched Readings", "matched_readings_top.png")

    if timeline_rows:
        x = [hour for hour, _ in timeline_rows]
        y = [int(count) for _, count in timeline_rows]
        plt.figure(figsize=(12, 6))
        plt.plot(x, y, marker="o")
        plt.title("Events per Hour (UTC)")
        plt.xticks(rotation=35, ha="right")
        plt.tight_layout()
        plt.savefig(out_dir / "events_per_hour.png", dpi=140)
        plt.close()


def main() -> int:
    args = parse_args()
    db_path = Path(args.db)
    out_dir = Path(args.out_dir)

    if not db_path.exists():
        print(f"[error] audit db not found: {db_path}")
        return 1

    out_dir.mkdir(parents=True, exist_ok=True)

    conn = sqlite3.connect(str(db_path))
    conn.row_factory = sqlite3.Row

    total_rows = conn.execute("SELECT COUNT(*) FROM audit_events").fetchone()[0]
    event_type = fetch_counter(
        conn,
        "SELECT event_type, COUNT(*) FROM audit_events GROUP BY event_type ORDER BY COUNT(*) DESC",
    )
    selected_action = fetch_counter(
        conn,
        "SELECT selected_action, COUNT(*) FROM audit_events GROUP BY selected_action ORDER BY COUNT(*) DESC",
    )
    suppressed_reason = fetch_counter(
        conn,
        "SELECT suppressed_reason, COUNT(*) FROM audit_events WHERE suppressed_reason <> '' GROUP BY suppressed_reason ORDER BY COUNT(*) DESC",
    )
    mode_counts = fetch_counter(
        conn,
        "SELECT mode, COUNT(*) FROM audit_events GROUP BY mode ORDER BY COUNT(*) DESC",
    )
    detector_backend = fetch_counter(
        conn,
        "SELECT detector_backend, COUNT(*) FROM audit_events GROUP BY detector_backend ORDER BY COUNT(*) DESC",
    )
    matched_readings = build_matched_reading_counter(conn)
    if args.max_top_readings > 0:
        matched_readings = Counter(dict(matched_readings.most_common(args.max_top_readings)))

    timeline_rows = conn.execute(
        "SELECT substr(ts_utc, 1, 13) || ':00Z' AS hour_utc, COUNT(*) "
        "FROM audit_events GROUP BY hour_utc ORDER BY hour_utc"
    ).fetchall()

    processing = build_processing_stats(conn)

    summary = {
        "db_path": str(db_path),
        "total_rows": int(total_rows),
        "event_type": dict(event_type),
        "selected_action": dict(selected_action),
        "suppressed_reason": dict(suppressed_reason),
        "mode": dict(mode_counts),
        "detector_backend": dict(detector_backend),
        "matched_readings_top": dict(matched_readings),
        "processing_time_ms": processing,
    }

    (out_dir / "summary.json").write_text(
        json.dumps(summary, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )

    write_counter_csv(out_dir / "event_type_counts.csv", ("event_type", "count"), event_type)
    write_counter_csv(
        out_dir / "response_action_counts.csv", ("selected_action", "count"), selected_action
    )
    write_counter_csv(
        out_dir / "suppression_reason_counts.csv", ("suppressed_reason", "count"), suppressed_reason
    )
    write_counter_csv(out_dir / "mode_counts.csv", ("mode", "count"), mode_counts)
    write_counter_csv(
        out_dir / "detector_backend_counts.csv", ("detector_backend", "count"), detector_backend
    )
    write_counter_csv(
        out_dir / "matched_readings_top.csv", ("matched_reading", "count"), matched_readings
    )
    write_timeline_csv(out_dir / "events_per_hour.csv", timeline_rows)

    if args.chart:
        maybe_generate_charts(
            out_dir,
            {
                "event_type": event_type,
                "selected_action": selected_action,
                "suppressed_reason": suppressed_reason,
                "mode": mode_counts,
                "matched_readings": matched_readings,
            },
            timeline_rows,
        )

    print(f"[ok] audit analytics written to {out_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
