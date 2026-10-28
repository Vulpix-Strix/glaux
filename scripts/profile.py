"""Run the profiling workloads and compare their reports.

Usage examples:
    mise run profile -- before
    mise run profile -- after
    mise run profile-compare -- before after

`run` plays each scenario of the ignored `profiling_workload` test twice, once
with `hotpath` for timing and once with `hotpath-alloc` for allocations, and
writes the JSON reports and the per-phase frame times to
target/profiling/<label>/. `compare` lists the functions whose p95 time or
allocated bytes moved by more than a threshold between two labels. See
PROFILING.md.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "target" / "profiling"
SCENARIOS = ("luau", "rhai", "editor", "paced", "analyzer")
UNITS = {"ns": 1e-9, "µs": 1e-6, "ms": 1e-3, "s": 1.0, "B": 1.0, "KB": 1e3, "MB": 1e6, "GB": 1e9}


def run(label: str) -> int:
    out = OUT / label
    out.mkdir(parents=True, exist_ok=True)
    for scenario in SCENARIOS:
        if scenario == "analyzer" and "LUAU_LSP" not in os.environ:
            print("analyzer: skipped, set LUAU_LSP to a luau-lsp executable", flush=True)
            continue
        for feature, suffix in (("hotpath", "time"), ("hotpath-alloc", "alloc")):
            report = out / f"{scenario}.{suffix}.json"
            env = {
                **os.environ,
                "GLAUX_WORKLOAD": scenario,
                "HOTPATH_METRICS_SERVER_OFF": "1",
                "HOTPATH_OUTPUT_FORMAT": "json-pretty",
                "HOTPATH_OUTPUT_PATH": str(report),
            }
            command = [
                "cargo", "nextest", "run", "--cargo-profile", "profiling", "-p", "glaux",
                "--features", feature, "--run-ignored", "only", "--no-capture",
                "-E", "test(profiling_workload::workload)",
            ]
            print(f"{scenario} ({feature})", flush=True)
            done = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True)
            if done.returncode != 0:
                sys.stderr.write(done.stdout + done.stderr)
                return done.returncode
            if feature == "hotpath":
                phases = [line for line in done.stderr.splitlines() if is_phase_line(line)]
                (out / f"{scenario}.phases.txt").write_text("\n".join(phases) + "\n")
    print(f"reports in {out.relative_to(ROOT)}")
    return 0


def is_phase_line(line: str) -> bool:
    fields = line.split()
    return line.startswith("phase ") or (len(fields) >= 7 and fields[-1].isdigit() and fields[-6].isdigit())


def value(text: str) -> float:
    number, _, unit = text.partition(" ")
    return float(number.replace(",", "")) * UNITS[unit]


def functions(path: Path, suffix: str) -> dict[str, float]:
    """The p95 time or the total allocated bytes of each reported function."""
    data = json.loads(path.read_text())
    key, column = ("functions_timing", "p95") if suffix == "time" else ("functions_alloc", "total")
    return {row["name"]: value(row[column]) for row in data[key]["data"]}


def compare(base: str, new: str, threshold: float) -> int:
    worse = 0
    for scenario in SCENARIOS:
        for suffix, floor, unit in (("time", 20e-6, "ms"), ("alloc", 10e3, "KB")):
            before_path = OUT / base / f"{scenario}.{suffix}.json"
            after_path = OUT / new / f"{scenario}.{suffix}.json"
            if not before_path.exists() or not after_path.exists():
                continue
            before, after = functions(before_path, suffix), functions(after_path, suffix)
            scale = UNITS[unit]
            print(f"\n{scenario} {suffix} ({'p95' if suffix == 'time' else 'total'}, {unit})")
            for name in sorted(before.keys() & after.keys()):
                if name.startswith("glaux::"):
                    continue
                old, now = before[name], after[name]
                if max(old, now) < floor:
                    continue
                change = (now - old) / old * 100 if old else float("inf")
                flag = ""
                if change > threshold and now - old > floor:
                    flag = "  <-- slower" if suffix == "time" else "  <-- more"
                    worse += 1
                print(f"  {name:<44} {old / scale:>10.3f} {now / scale:>10.3f} {change:>+8.1f}%{flag}")
    print(f"\n{worse} function(s) above +{threshold:.0f}%")
    return 1 if worse else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("run").add_argument("label")
    cmp = commands.add_parser("compare")
    cmp.add_argument("base")
    cmp.add_argument("new")
    cmp.add_argument("--threshold", type=float, default=25.0, help="percent change that counts (default 25)")
    args = parser.parse_args()
    return run(args.label) if args.command == "run" else compare(args.base, args.new, args.threshold)


if __name__ == "__main__":
    sys.exit(main())
