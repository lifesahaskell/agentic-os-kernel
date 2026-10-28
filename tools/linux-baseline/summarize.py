#!/usr/bin/env python3
"""Aggregate a Linux baseline run directory into summary.md and summary.csv.

Run with: python3 -I tools/linux-baseline/summarize.py RUN_DIR
Check:    python3 -I tools/linux-baseline/summarize.py --check RUN_DIR

Standard library only. Definitions (per suite, config, bench, over rounds):
  med_p50      median of the round p50 values (likewise med_p90, med_p99, med_p999)
  spread_p50   100 * (max round p50 - min round p50) / med_p50 (likewise p99)
  n_total      sum of n over rounds; min and max are taken over rounds
  added_*      against the stated baseline; within_noise when the [min, max] ranges of the
               round p50 values of config and baseline overlap
  stability    stable below 10 percent spread, unstable 10 to 25, noise-dominated above 25
"""

import csv
import os
import statistics
import sys

SUITES = ("micro", "broker", "deadline", "cgroup")

MICRO_BENCHES = ("timer_pair", "getppid", "getppid_x1000")

MICRO_CONFIGS = ("C0", "C1a", "C1b", "C1c", "C2a", "C2b", "C3")
BROKER_CONFIGS = (
    "direct",
    "broker_noid",
    "broker_id",
    "egress_direct",
    "egress_broker",
)
DEADLINE_PIDFD = ("pidfd_L0", "pidfd_L1", "pidfd_L4")
DEADLINE_CGKILL = ("cgkill_L0", "cgkill_L1", "cgkill_L4")
CGROUP_CONFIGS = ("spawn_fork", "spawn_clone3_into", "spawn_fork_move")

BROKER_BENCHES = ("roundtrip",)
DEADLINE_BENCHES = ("deadline_to_wakeup", "wakeup_to_reaped", "deadline_to_reaped")
DEADLINE_CGKILL_EXTRA = ("deadline_to_empty",)
CGROUP_BENCHES = ("spawn_exec_wait",)

# L12 plausibility windows (ASSUMED), nanoseconds, from the planner probe.
WINDOWS = {("C0", "getppid"): (60.0, 400.0), ("C0", "openat_tmpfs"): (800.0, 5000.0)}

BROKER_BASELINE = {
    "broker_noid": "direct",
    "broker_id": "direct",
    "egress_broker": "egress_direct",
}
CGROUP_BASELINE = {"spawn_clone3_into": "spawn_fork", "spawn_fork_move": "spawn_fork"}

BENCH_ORDER = {}

FIELDS = [
    "suite",
    "config",
    "bench",
    "rounds",
    "n_total",
    "min_ns",
    "med_p50_ns",
    "spread_p50_pct",
    "med_p90_ns",
    "med_p99_ns",
    "spread_p99_pct",
    "med_p999_ns",
    "max_ns",
    "baseline",
    "added_p50_ns",
    "added_p50_pct",
    "added_p99_ns",
    "added_p99_pct",
    "within_noise",
    "stability",
    "errors",
]


def read_kv(path):
    """key=value lines into dict key -> list of values."""
    out = {}
    if not os.path.exists(path):
        return out
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.rstrip("\n")
            if "=" not in line or line.startswith("#"):
                continue
            k, v = line.split("=", 1)
            out.setdefault(k, []).append(v)
    return out


def parse_rounds(spec):
    rounds = set()
    for part in spec.split(","):
        part = part.strip()
        if not part:
            continue
        if "-" in part:
            a, b = part.split("-", 1)
            rounds.update(range(int(a), int(b) + 1))
        else:
            rounds.add(int(part))
    return rounds


def load_rows(run_dir):
    rows = []
    for suite in SUITES:
        path = os.path.join(run_dir, suite + ".csv")
        if not os.path.exists(path):
            continue
        with open(path, newline="", encoding="utf-8") as f:
            for r in csv.DictReader(f):
                rows.append(r)
    return rows


def pct_spread(vals, med):
    if med == 0:
        return 0.0
    return 100.0 * (max(vals) - min(vals)) / med


def stability(spread):
    if spread < 10.0:
        return "stable"
    if spread <= 25.0:
        return "unstable"
    return "noise-dominated"


def aggregate(rows):
    """(suite, config, bench) -> aggregate dict."""
    groups = {}
    for r in rows:
        BENCH_ORDER.setdefault(r["bench"], len(BENCH_ORDER))
        groups.setdefault((r["suite"], r["config"], r["bench"]), []).append(r)
    agg = {}
    for key, rs in groups.items():
        p50 = [float(r["p50_ns"]) for r in rs]
        p90 = [float(r["p90_ns"]) for r in rs]
        p99 = [float(r["p99_ns"]) for r in rs]
        p999 = [float(r["p999_ns"]) for r in rs]
        m50 = statistics.median(p50)
        m99 = statistics.median(p99)
        agg[key] = {
            "rounds": len({int(r["round"]) for r in rs}),
            "n_total": sum(int(r["n"]) for r in rs),
            "min": min(float(r["min_ns"]) for r in rs),
            "max": max(float(r["max_ns"]) for r in rs),
            "med_p50": m50,
            "med_p90": statistics.median(p90),
            "med_p99": m99,
            "med_p999": statistics.median(p999),
            "spread_p50": pct_spread(p50, m50),
            "spread_p99": pct_spread(p99, m99),
            "p50_range": (min(p50), max(p50)),
            "errors": sum(int(r["errors"]) for r in rs),
        }
    return agg


def baseline_of(suite, config):
    if suite == "micro":
        return None if config == "C0" else "C0"
    if suite == "broker":
        return BROKER_BASELINE.get(config)
    if suite == "cgroup":
        return CGROUP_BASELINE.get(config)
    return None


def added(a, b):
    """Added overhead of aggregate a against baseline aggregate b."""
    d50 = a["med_p50"] - b["med_p50"]
    d99 = a["med_p99"] - b["med_p99"]
    p50pct = 100.0 * d50 / b["med_p50"] if b["med_p50"] else 0.0
    p99pct = 100.0 * d99 / b["med_p99"] if b["med_p99"] else 0.0
    lo = max(a["p50_range"][0], b["p50_range"][0])
    hi = min(a["p50_range"][1], b["p50_range"][1])
    return {
        "added_p50_ns": d50,
        "added_p50_pct": p50pct,
        "added_p99_ns": d99,
        "added_p99_pct": p99pct,
        "within_noise": "yes" if lo <= hi else "no",
    }


def build_table(agg):
    out = []
    for key in sorted(agg, key=sort_key):
        suite, config, bench = key
        a = agg[key]
        base = baseline_of(suite, config)
        row = {
            "suite": suite,
            "config": config,
            "bench": bench,
            "rounds": a["rounds"],
            "n_total": a["n_total"],
            "min_ns": a["min"],
            "med_p50_ns": a["med_p50"],
            "spread_p50_pct": a["spread_p50"],
            "med_p90_ns": a["med_p90"],
            "med_p99_ns": a["med_p99"],
            "spread_p99_pct": a["spread_p99"],
            "med_p999_ns": a["med_p999"],
            "max_ns": a["max"],
            "baseline": base or "",
            "added_p50_ns": "",
            "added_p50_pct": "",
            "added_p99_ns": "",
            "added_p99_pct": "",
            "within_noise": "",
            "stability": stability(a["spread_p50"]),
            "errors": a["errors"],
        }
        if base and (suite, base, bench) in agg:
            row.update(added(a, agg[(suite, base, bench)]))
        out.append(row)
        if (
            suite == "broker"
            and config == "broker_id"
            and ("broker", "broker_noid", bench) in agg
        ):
            d = dict(row)
            d["config"] = "broker_id_vs_noid"
            d["baseline"] = "broker_noid"
            d.update(added(a, agg[("broker", "broker_noid", bench)]))
            out.append(d)
    return out


def sort_key(key):
    suite, config, bench = key
    order = {
        "micro": MICRO_CONFIGS,
        "broker": BROKER_CONFIGS,
        "deadline": DEADLINE_PIDFD + DEADLINE_CGKILL,
        "cgroup": CGROUP_CONFIGS,
    }[suite]
    base = config.split("-")[0]
    ci = order.index(base) if base in order else len(order)
    return (SUITES.index(suite), ci, config, BENCH_ORDER.get(bench, 99))


def fmt_ns(v):
    return "%.3f" % v if v != "" else "-"


def fmt_pct(v):
    return "%.2f" % v if v != "" else "-"


def csv_value(field, v):
    if v == "":
        return ""
    if field.endswith("_pct"):
        return "%.2f" % v
    if field.endswith("_ns"):
        return "%.3f" % v
    return str(v)


def md_table(rows):
    head = (
        "| config | bench | rounds | n_total | min | med_p50 | spread_p50 % | med_p90 | med_p99 "
        "| spread_p99 % | med_p999 | max | baseline | added p50 ns | added p50 % | added p99 ns "
        "| added p99 % | within_noise | stability |"
    )
    sep = "|" + "---|" * 19
    lines = [head, sep]
    for r in rows:
        lines.append(
            "| {config} | {bench} | {rounds} | {n_total} | {mn} | {m50} | {s50} | {m90} | {m99} "
            "| {s99} | {m999} | {mx} | {base} | {a50n} | {a50p} | {a99n} | {a99p} | {wn} "
            "| {st} |".format(
                config=r["config"],
                bench=r["bench"],
                rounds=r["rounds"],
                n_total=r["n_total"],
                mn=fmt_ns(r["min_ns"]),
                m50=fmt_ns(r["med_p50_ns"]),
                s50=fmt_pct(r["spread_p50_pct"]),
                m90=fmt_ns(r["med_p90_ns"]),
                m99=fmt_ns(r["med_p99_ns"]),
                s99=fmt_pct(r["spread_p99_pct"]),
                m999=fmt_ns(r["med_p999_ns"]),
                mx=fmt_ns(r["max_ns"]),
                base=r["baseline"] or "-",
                a50n=fmt_ns(r["added_p50_ns"]),
                a50p=fmt_pct(r["added_p50_pct"]),
                a99n=fmt_ns(r["added_p99_ns"]),
                a99p=fmt_pct(r["added_p99_pct"]),
                wn=r["within_noise"] or "-",
                st=r["stability"],
            )
        )
    return "\n".join(lines)


def verdict_block(table, agg, run_dir, probe_lines):
    out = ["## Verdict", ""]
    c0 = [r for r in table if r["suite"] == "micro" and r["config"] == "C0"]
    if c0:
        counts = {"stable": 0, "unstable": 0, "noise-dominated": 0}
        for r in c0:
            counts[r["stability"]] += 1
        out.append(
            "C0 micro stability (spread of round p50, %d benches): stable %d, unstable %d, "
            "noise-dominated %d."
            % (len(c0), counts["stable"], counts["unstable"], counts["noise-dominated"])
        )
        for label in ("unstable", "noise-dominated"):
            names = [r["bench"] for r in c0 if r["stability"] == label]
            out.append("- %s: %s" % (label, ", ".join(names) if names else "none"))
        out.append("")
        out.append("Plausibility windows (ASSUMED, from the planner probe):")
        for (cfg, bench), (lo, hi) in WINDOWS.items():
            a = agg.get(("micro", cfg, bench))
            if a is None:
                out.append("- %s %s: no data" % (cfg, bench))
            else:
                ok = "yes" if lo <= a["med_p50"] <= hi else "no"
                out.append(
                    "- %s %s med_p50 %.3f ns, window %.0f to %.0f ns, inside: %s"
                    % (cfg, bench, a["med_p50"], lo, hi, ok)
                )
    else:
        out.append("No C0 micro rows in this run directory.")
    out.append("")
    other = []
    for suite, bases in (
        ("broker", ("direct", "egress_direct")),
        ("cgroup", ("spawn_fork",)),
    ):
        for r in table:
            if r["suite"] == suite and r["config"] in bases:
                other.append(
                    "- %s %s %s: spread_p50 %.2f %%, %s"
                    % (
                        suite,
                        r["config"],
                        r["bench"],
                        r["spread_p50_pct"],
                        r["stability"],
                    )
                )
    if other:
        out.append("Baseline rows of the other suites:")
        out.extend(other)
        out.append("")
    nm = [ln for ln in probe_lines if ln.startswith("NOT MEASURED")]
    out.append(
        "NOT MEASURED legs from cgroup-probe.txt: %s" % ("none" if not nm else "")
    )
    for ln in nm:
        out.append("- " + ln)
    out.append("")
    return "\n".join(out)


def read_lines(path):
    if not os.path.exists(path):
        return []
    with open(path, encoding="utf-8") as f:
        return [ln.rstrip("\n") for ln in f if ln.strip()]


def behaviour_table(lines):
    if not lines:
        return ""
    out = ["## cgroup behaviour legs", "", "| leg | fields |", "|---|---|"]
    for ln in lines:
        toks = ln.split()
        out.append("| %s | %s |" % (toks[0].split("=", 1)[0], " ".join(toks)))
    out.append("")
    return "\n".join(out)


def summarize(run_dir):
    rows = load_rows(run_dir)
    agg = aggregate(rows)
    table = build_table(agg)
    probe_lines = read_lines(os.path.join(run_dir, "cgroup-probe.txt"))
    with open(
        os.path.join(run_dir, "summary.csv"), "w", newline="", encoding="utf-8"
    ) as f:
        w = csv.writer(f)
        w.writerow(FIELDS)
        for r in table:
            w.writerow([csv_value(k, r[k]) for k in FIELDS])
    md = [
        "# Linux baseline summary",
        "",
        "Run directory: `%s`" % os.path.basename(run_dir.rstrip("/")),
        "",
        verdict_block(table, agg, run_dir, probe_lines),
    ]
    titles = {
        "micro": "Microbenchmarks (S3)",
        "broker": "Broker round trip (S4)",
        "deadline": "Deadline enforcement (S6, CLOCK_MONOTONIC)",
        "cgroup": "cgroup spawn legs",
    }
    for suite in SUITES:
        sub = [r for r in table if r["suite"] == suite]
        if sub:
            md.append("## %s" % titles[suite])
            md.append("")
            md.append(md_table(sub))
            md.append("")
    if probe_lines:
        md.append("## cgroup probe")
        md.append("")
        md.append("```")
        md.extend(probe_lines)
        md.append("```")
        md.append("")
    md.append(
        behaviour_table(read_lines(os.path.join(run_dir, "cgroup-behaviour.txt")))
    )
    with open(os.path.join(run_dir, "summary.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(md) + "\n")


def expected_benches(suite, config):
    if suite == "micro":
        return MICRO_BENCHES
    if suite == "broker":
        return BROKER_BENCHES
    if suite == "deadline":
        extra = DEADLINE_CGKILL_EXTRA if config.startswith("cgkill") else ()
        return DEADLINE_BENCHES + extra
    return CGROUP_BENCHES


def check(run_dir):
    problems = []
    env = read_kv(os.path.join(run_dir, "env.txt"))
    if not env:
        problems.append("env.txt missing or empty")
    rows = load_rows(run_dir)
    have = {(r["suite"], r["config"], int(r["round"]), r["bench"]) for r in rows}

    sc_lines = read_lines(os.path.join(run_dir, "selfcheck.txt"))
    sc = {}
    for ln in sc_lines:
        toks = ln.split()
        d = dict(t.split("=", 1) for t in toks[1:] if "=" in t)
        sc[(toks[0], int(d.get("round", "0")))] = d.get("result", "?")
        if d.get("result") != "ok":
            problems.append("selfcheck not ok: " + ln)
    if not sc_lines:
        problems.append("selfcheck.txt missing or empty")

    probe = read_kv(os.path.join(run_dir, "cgroup-probe.txt"))
    probe_lines = read_lines(os.path.join(run_dir, "cgroup-probe.txt"))
    delegation = (probe.get("delegation") or [""])[0]

    asked = False
    for suite in SUITES:
        cfgs = set()
        for v in env.get("requested_%s_configs" % suite, []):
            cfgs.update(c for c in v.replace(",", " ").split() if c)
        rnds = set()
        for v in env.get("requested_%s_rounds" % suite, []):
            rnds |= parse_rounds(v)
        if not cfgs and not rnds:
            continue
        asked = True
        for cfg in sorted(cfgs):
            for rnd in sorted(rnds):
                for bench in expected_benches(suite, cfg):
                    if (suite, cfg, rnd, bench) not in have:
                        problems.append(
                            "missing row %s %s round %d %s" % (suite, cfg, rnd, bench)
                        )
                if (cfg, rnd) not in sc:
                    problems.append("missing selfcheck line %s round %d" % (cfg, rnd))
        if suite == "deadline" and delegation == "yes":
            for c in DEADLINE_CGKILL:
                if c not in cfgs:
                    problems.append("delegation=yes but %s not requested" % c)
    if not asked:
        problems.append("env.txt lists no requested_* suites")
    if "cgroup" in env.get("requested_cgroup_configs", []) or probe:
        if delegation == "no" and not any(
            ln.startswith("NOT MEASURED") for ln in probe_lines
        ):
            problems.append(
                "delegation=no but no NOT MEASURED lines in cgroup-probe.txt"
            )
        if delegation == "yes" and not (
            set(CGROUP_CONFIGS) <= {r["config"] for r in rows if r["suite"] == "cgroup"}
        ):
            problems.append("delegation=yes but cgroup spawn rows are missing")
    return problems


def main(argv):
    if len(argv) == 3 and argv[1] == "--check":
        problems = check(argv[2])
        if problems:
            for p in problems:
                print("CHECK-FAIL: " + p)
            return 1
        print("CHECK-OK")
        return 0
    if len(argv) == 2 and not argv[1].startswith("-"):
        summarize(argv[1])
        return 0
    print(__doc__)
    return 64


if __name__ == "__main__":
    sys.exit(main(sys.argv))
