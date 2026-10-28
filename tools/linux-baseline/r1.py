#!/usr/bin/env python3
"""R1 threshold calculator for the Linux baseline (linux-baseline.md section 11).

Run with: python3 -I tools/linux-baseline/r1.py RUN_DIR [--config CFG] [--against C0|C3]
BPF-LSM:  python3 -I tools/linux-baseline/r1.py --bpflsm BASE_RUN_DIR VARIANT_RUN_DIR...
Check:    python3 -I tools/linux-baseline/r1.py --self-test

Standard library only. Output goes to stdout. It is informational under D-18: the S3 bound in
scoping-linux.md is not revised by this script, and nothing here writes under docs/.

Rules (section 11, interpretation):
  eligibility  a leg yields a number only when the row is marked stable by summarize.py and both
               the baseline and the config spread on the quantile the bound constrains
               (spread_p50 for p50, spread_p99 for p99) are strictly below 10 percent. Spread
               exactly 10.00 is ineligible. Otherwise the leg reads "none: unstable on this host".
  R1           max(current bound, added + 2 x the larger of the two spreads), the sum rounded UP
               to a whole percent (Decimal ROUND_CEILING). When the rounded sum does not exceed
               the bound the leg reads "keep <bound> (<sum>)"; a bound is never lowered.
  added        the default read is the added_*_pct column of summary.csv (config C3 against C0).
               With --against C3, and always in --bpflsm mode, added = 100*(a-b)/b computed from
               the med_*_ns strings in Decimal, quantized to 0.01 with ROUND_HALF_EVEN.
All numbers are parsed with Decimal from the csv strings; no float arithmetic touches a result.
"""

import contextlib
import csv
import importlib.util
import io
import os
import sys
import tempfile
from decimal import (
    ROUND_CEILING,
    ROUND_HALF_EVEN,
    Decimal,
    DivisionByZero,
    InvalidOperation,
)

HERE = os.path.dirname(os.path.abspath(__file__))
COMMITTED_RUN = os.path.join(HERE, "results", "20261028T034700Z")

# The seven S3 legs, and the current proposed bounds in percent (scoping-linux.md S3).
S3_BENCHES = (
    "openat_tmpfs",
    "openat_rootfs",
    "unix_connect",
    "tcp_connect",
    "fork_exec_wait",
    "posix_spawn",
    "unix_dgram_pingpong",
)
QUANTILES = ("p50", "p99")
BOUNDS = {"p50": Decimal(10), "p99": Decimal(25)}

REQUIRED_VARIANTS = ("global-miss", "global-hit", "global-miss-nocount")
OPTIONAL_VARIANTS = ("cg", "cgsib", "cgroot", "anc")
VARIANT_PREFIX = "C3-bpflsm-"

TEN = Decimal(10)
CENT = Decimal("0.01")
BPFLSM_FIRST_LINE = "informational under D-18; the S3 bound is not revised"
CONTRACT_LINE = "added = 100*(a-b)/b from the med_*_ns strings in Decimal, quantized to 0.01 ROUND_HALF_EVEN"
UNSTABLE = "none: unstable on this host"


def load_summarize():
    """Load summarize.py from this directory. python3 -I keeps the script directory off sys.path."""
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location(
        "aok_summarize", os.path.join(HERE, "summarize.py")
    )
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


SUMMARIZE = load_summarize()


def dec(s):
    """Parse a csv percent or ns string; an empty or malformed string is an error."""
    if s is None or str(s).strip() == "":
        raise ValueError("empty numeric field")
    try:
        return Decimal(str(s).strip())
    except InvalidOperation:
        raise ValueError("not a number: %r" % (s,))


def load_summary(run_dir):
    """(suite, config, bench) -> csv row dict, from RUN_DIR/summary.csv."""
    path = os.path.join(run_dir, "summary.csv")
    out = {}
    with open(path, newline="", encoding="utf-8") as f:
        for r in csv.DictReader(f):
            out[(r["suite"], r["config"], r["bench"])] = r
    return out


def micro(table, config, bench):
    return table.get(("micro", config, bench))


def is_eligible(row, q):
    """The shared D-18 eligibility rule (also used by transfer.py)."""
    if row is None:
        return False
    mark = row.get("stability") or SUMMARIZE.stability(
        float(dec(row["spread_p50_pct"]))
    )
    if mark != "stable":
        return False
    return dec(row["spread_%s_pct" % q]) < TEN


def added_from_med(a_row, b_row, q):
    """The --against contract: 100*(a-b)/b from med_*_ns, quantized 0.01 ROUND_HALF_EVEN."""
    a = dec(a_row["med_%s_ns" % q])
    b = dec(b_row["med_%s_ns" % q])
    return (Decimal(100) * (a - b) / b).quantize(CENT, rounding=ROUND_HALF_EVEN)


def ceil_int(x):
    return x.to_integral_value(rounding=ROUND_CEILING)


class Result:
    """One leg verdict: kind is 'R1', 'keep' or 'none'."""

    def __init__(self, kind, bound=None, total=None, rounded=None):
        self.kind = kind
        self.bound = bound
        self.total = total
        self.rounded = rounded

    def text(self):
        if self.kind == "none":
            return UNSTABLE
        if self.kind == "keep":
            return "keep %s (%s)" % (self.bound, self.total)
        return "R1 %s (%s)" % (self.rounded, self.total)


def r1_leg(q, base_row, cfg_row, added):
    """R1 for one leg and quantile. added is a Decimal percent; None when it cannot be had."""
    if added is None or not is_eligible(base_row, q) or not is_eligible(cfg_row, q):
        return Result("none")
    larger = max(dec(base_row["spread_%s_pct" % q]), dec(cfg_row["spread_%s_pct" % q]))
    total = (added + 2 * larger).quantize(CENT, rounding=ROUND_HALF_EVEN)
    rounded = ceil_int(total)
    bound = BOUNDS[q]
    if rounded <= bound:
        return Result("keep", bound=bound, total=total)
    return Result("R1", total=total, rounded=rounded)


def base_report(table, subject, against):
    """(bench, q) -> Result for the seven S3 legs: subject config against the baseline config."""
    out = {}
    for bench in S3_BENCHES:
        b = micro(table, against, bench)
        c = micro(table, subject, bench)
        for q in QUANTILES:
            if b is None or c is None:
                out[(bench, q)] = None
                continue
            if (
                against == "C0"
                and c.get("baseline") == "C0"
                and c.get("added_%s_pct" % q)
            ):
                added = dec(c["added_%s_pct" % q])
            else:
                added = added_from_med(c, b, q)
            out[(bench, q)] = r1_leg(q, b, c, added)
    return out


def print_base(table, subject, against):
    print(
        "R1 for %s against %s (informational under D-18; the S3 bound is not revised)"
        % (subject, against)
    )
    if against == "C0":
        print("added is the added_*_pct column of summary.csv")
    else:
        print(CONTRACT_LINE)
    rep = base_report(table, subject, against)
    for q in QUANTILES:
        for bench in S3_BENCHES:
            r = rep[(bench, q)]
            if r is None:
                print("%s %s: missing: no %s or %s row" % (bench, q, subject, against))
            else:
                print("%s %s: %s" % (bench, q, r.text()))


def collect_variants(variant_dirs):
    """config -> {bench: row} for every C3-bpflsm-* config across the variant directories."""
    found = {}
    owner = {}
    for d in variant_dirs:
        for (suite, config, bench), row in load_summary(d).items():
            if suite != "micro" or not config.startswith(VARIANT_PREFIX):
                continue
            if config in owner and owner[config] != d:
                raise ValueError(
                    "%s appears in both %s and %s; pass each variant from one directory"
                    % (config, owner[config], d)
                )
            owner[config] = d
            found.setdefault(config, {})[bench] = row
    return found


def variant_order(found):
    names = [VARIANT_PREFIX + v for v in REQUIRED_VARIANTS]
    names += [
        VARIANT_PREFIX + v for v in OPTIONAL_VARIANTS if VARIANT_PREFIX + v in found
    ]
    known = set(names)
    names += sorted(c for c in found if c not in known)
    return names


def bpflsm_report(base_table, found):
    """list of (config, bench, q, text, added_c0, added_c3) for the BPF-LSM variants."""
    rows = []
    for config in variant_order(found):
        for bench in S3_BENCHES:
            c0 = micro(base_table, "C0", bench)
            c3 = micro(base_table, "C3", bench)
            if c0 is None and c3 is None:
                continue
            var = found.get(config, {}).get(bench)
            for q in QUANTILES:
                if var is None:
                    rows.append(
                        (config, bench, q, "missing: no %s row" % config, None, None)
                    )
                    continue
                if c0 is None or c3 is None:
                    rows.append(
                        (
                            config,
                            bench,
                            q,
                            "missing: no %s row in the base run"
                            % ("C0" if c0 is None else "C3"),
                            None,
                            None,
                        )
                    )
                    continue
                vs_c0 = added_from_med(var, c0, q)
                vs_c3 = added_from_med(var, c3, q)
                res = r1_leg(q, c0, var, vs_c0)
                rows.append((config, bench, q, res.text(), vs_c0, vs_c3))
    return rows


def print_bpflsm(base_dir, variant_dirs):
    print(BPFLSM_FIRST_LINE)
    print(CONTRACT_LINE)
    print(
        "R1 here is max(bound, ceil(added vs C0 + 2 x larger spread of the C0 row and the variant row))"
    )
    base = load_summary(base_dir)
    found = collect_variants(variant_dirs)
    for config, bench, q, text, vs_c0, vs_c3 in bpflsm_report(base, found):
        if vs_c0 is None:
            print("%s %s %s: %s" % (config, bench, q, text))
        else:
            print(
                "%s %s %s: %s; added vs C0 %s, added vs C3 %s"
                % (config, bench, q, text, vs_c0, vs_c3)
            )


# ---- self-test --------------------------------------------------------------------------------

EXPECT_P50 = {
    "openat_tmpfs": "R1 45 (44.29)",
    "openat_rootfs": "R1 19 (18.90)",
    "unix_connect": "keep 10 (4.16)",
    "tcp_connect": "keep 10 (3.97)",
    "fork_exec_wait": "keep 10 (3.43)",
    "posix_spawn": "keep 10 (3.30)",
    "unix_dgram_pingpong": "R1 14 (13.53)",
}
EXPECT_P99 = {
    "openat_tmpfs": UNSTABLE,
    "openat_rootfs": UNSTABLE,
    "unix_connect": "keep 25 (9.87)",
    "tcp_connect": "keep 25 (8.81)",
    "fork_exec_wait": "keep 25 (11.31)",
    "posix_spawn": UNSTABLE,
    "unix_dgram_pingpong": "keep 25 (16.44)",
}


def fixture_row(config, bench, p50, s50, p99, s99):
    row = dict.fromkeys(SUMMARIZE.FIELDS, "")
    row.update(
        suite="micro",
        config=config,
        bench=bench,
        med_p50_ns=p50,
        spread_p50_pct=s50,
        med_p99_ns=p99,
        spread_p99_pct=s99,
        stability=SUMMARIZE.stability(float(Decimal(s50))),
    )
    return row


def write_fixture(path, rows):
    os.makedirs(path)
    with open(
        os.path.join(path, "summary.csv"), "w", newline="", encoding="utf-8"
    ) as f:
        w = csv.DictWriter(f, fieldnames=SUMMARIZE.FIELDS)
        w.writeheader()
        for r in rows:
            w.writerow(r)


def self_test_base(fails):
    table = load_summary(COMMITTED_RUN)
    rep = base_report(table, "C3", "C0")
    for q, expect in (("p50", EXPECT_P50), ("p99", EXPECT_P99)):
        for bench, want in expect.items():
            got = rep[(bench, q)].text() if rep[(bench, q)] else "missing"
            if got != want:
                fails.append("base %s %s: got %r, want %r" % (bench, q, got, want))
    # The contract path must agree with the csv column on this run.
    for bench in S3_BENCHES:
        for q in QUANTILES:
            c3 = micro(table, "C3", bench)
            got = added_from_med(c3, micro(table, "C0", bench), q)
            col = dec(c3["added_%s_pct" % q])
            if got != col:
                fails.append(
                    "contract %s %s: %s differs from csv %s" % (bench, q, got, col)
                )
    cm = added_from_med(
        micro(table, "C3", "openat_tmpfs"), micro(table, "C0", "openat_tmpfs"), "p50"
    )
    if cm != Decimal("25.01"):
        fails.append("contract added openat_tmpfs p50: got %s, want 25.01" % cm)


def self_test_bpflsm(fails):
    with tempfile.TemporaryDirectory() as tmp:
        base = os.path.join(tmp, "base")
        write_fixture(
            base,
            [
                fixture_row(
                    "C0", "openat_tmpfs", "1000.000", "2.00", "2000.000", "2.00"
                ),
                fixture_row(
                    "C3", "openat_tmpfs", "1050.000", "2.00", "2050.000", "2.00"
                ),
                fixture_row(
                    "C0", "openat_rootfs", "1000.000", "2.00", "2000.000", "2.00"
                ),
                fixture_row(
                    "C3", "openat_rootfs", "1050.000", "2.00", "2050.000", "2.00"
                ),
            ],
        )
        va = os.path.join(tmp, "a")
        vb = os.path.join(tmp, "b")
        vc = os.path.join(tmp, "c")
        write_fixture(
            va,
            [
                fixture_row(
                    "C3-bpflsm-global-miss",
                    "openat_tmpfs",
                    "1100.000",
                    "3.00",
                    "2100.000",
                    "2.00",
                ),
                fixture_row(
                    "C3-bpflsm-global-miss",
                    "openat_rootfs",
                    "1100.000",
                    "10.00",
                    "2100.000",
                    "2.00",
                ),
            ],
        )
        write_fixture(
            vb,
            [
                fixture_row(
                    "C3-bpflsm-global-hit",
                    "openat_tmpfs",
                    "1150.000",
                    "2.00",
                    "2100.000",
                    "2.00",
                )
            ],
        )
        write_fixture(
            vc,
            [
                fixture_row(
                    "C3-bpflsm-global-miss-nocount",
                    "openat_tmpfs",
                    "1050.000",
                    "2.00",
                    "2100.000",
                    "2.00",
                )
            ],
        )
        base_table = load_summary(base)
        rep = {}
        for config, bench, q, text, vs_c0, vs_c3 in bpflsm_report(
            base_table, collect_variants([va, vb, vc])
        ):
            rep[(config, bench, q)] = (text, vs_c0, vs_c3)

        def want(key, text, vs_c0=None, vs_c3=None):
            got = rep.get(key)
            if got is None:
                fails.append("bpflsm %r: no line" % (key,))
                return
            if got[0] != text:
                fails.append("bpflsm %r: got %r, want %r" % (key, got[0], text))
            if vs_c0 is not None and got[1] != Decimal(vs_c0):
                fails.append(
                    "bpflsm %r: added vs C0 %s, want %s" % (key, got[1], vs_c0)
                )
            if vs_c3 is not None and got[2] != Decimal(vs_c3):
                fails.append(
                    "bpflsm %r: added vs C3 %s, want %s" % (key, got[2], vs_c3)
                )

        m = VARIANT_PREFIX + "global-miss"
        h = VARIANT_PREFIX + "global-hit"
        n = VARIANT_PREFIX + "global-miss-nocount"
        want((m, "openat_tmpfs", "p50"), "R1 16 (16.00)", "10.00", "4.76")
        want((h, "openat_tmpfs", "p50"), "R1 19 (19.00)", "15.00", "9.52")
        want((n, "openat_tmpfs", "p50"), "keep 10 (9.00)")
        for v in (m, h, n):
            want((v, "openat_tmpfs", "p99"), "keep 25 (9.00)")
        want((m, "openat_rootfs", "p50"), UNSTABLE)
        want((h, "openat_rootfs", "p50"), "missing: no %s row" % h)

        # Second pass over dirs A and C only: the hit variant is absent and the others still print.
        rep2 = bpflsm_report(base_table, collect_variants([va, vc]))
        by_key = {(c, b, q): t for c, b, q, t, _, _ in rep2}
        if by_key.get((h, "openat_tmpfs", "p50")) != "missing: no %s row" % h:
            fails.append("bpflsm second pass: hit variant not reported missing")
        if not by_key.get((m, "openat_tmpfs", "p50"), "").startswith("R1 16"):
            fails.append("bpflsm second pass: global-miss line absent")
        if not by_key.get((n, "openat_tmpfs", "p50"), "").startswith("keep 10"):
            fails.append("bpflsm second pass: nocount line absent")

        # The rendered output starts with the informational line and states the contract.
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            print_bpflsm(base, [va, vb, vc])
        lines = buf.getvalue().splitlines()
        if not lines or lines[0] != BPFLSM_FIRST_LINE:
            fails.append("bpflsm output does not start with %r" % BPFLSM_FIRST_LINE)
        if CONTRACT_LINE not in lines:
            fails.append("bpflsm output does not state the --against contract")


def self_test():
    fails = []
    self_test_base(fails)
    self_test_bpflsm(fails)
    table = load_summary(COMMITTED_RUN)
    rep = base_report(table, "C3", "C0")
    for q in QUANTILES:
        for bench in S3_BENCHES:
            print("%s %s: %s" % (bench, q, rep[(bench, q)].text()))
    if fails:
        for f in fails:
            print("CHECK-FAIL: " + f)
        return 1
    print("CHECK-OK")
    return 0


# ---- entry point ------------------------------------------------------------------------------


def main(argv):
    args = argv[1:]
    try:
        if args == ["--self-test"]:
            return self_test()
        if args and args[0] == "--bpflsm":
            if len(args) < 3:
                print(__doc__, file=sys.stderr)
                return 64
            print_bpflsm(args[1], args[2:])
            return 0
        if args and not args[0].startswith("-"):
            run_dir = args[0]
            subject, against = "C3", "C0"
            rest = args[1:]
            while rest:
                if (
                    rest[0] == "--against"
                    and len(rest) >= 2
                    and rest[1] in ("C0", "C3")
                ):
                    against = rest[1]
                    rest = rest[2:]
                elif rest[0] == "--config" and len(rest) >= 2:
                    subject = rest[1]
                    rest = rest[2:]
                else:
                    print(__doc__, file=sys.stderr)
                    return 64
            print_base(load_summary(run_dir), subject, against)
            return 0
    except (OSError, ValueError, KeyError, DivisionByZero, InvalidOperation) as e:
        print("r1.py: %s" % (e,), file=sys.stderr)
        return 65
    print(__doc__, file=sys.stderr)
    return 64


if __name__ == "__main__":
    sys.exit(main(sys.argv))
