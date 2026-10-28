#!/usr/bin/env python3
"""HARN-01 per-leg transfer verdict between two Linux baseline run directories.

Run with: python3 -I tools/linux-baseline/transfer.py OLD_RUN_DIR NEW_RUN_DIR
Check:    python3 -I tools/linux-baseline/transfer.py --self-test

Standard library only; numbers are parsed with Decimal. Each leg gets one verdict:
  transfers              every row at the quantile is eligible and the delta is at most the
                         tolerance (a delta exactly equal to the tolerance transfers)
  does not transfer      every row is eligible and the delta exceeds the tolerance
  unreadable (unstable)  any row is ineligible: not marked stable by summarize.py, or its spread
                         on the quantile is 10.00 or greater (the r1.py eligibility rule)
Tolerance is 2 x the largest spread, on the quantile, among the rows the leg compares.

Legs:
  S3  seven micro benches, C3 against C0, p50 and p99 separately. Four rows per leg and quantile
      (old C0, old C3, new C0, new C3). Delta is the absolute difference of the two added
      percents, each computed with the r1.py --against contract.
  S4  broker_id against direct and egress_broker against egress_direct, at p99. Four rows per
      leg. The value is the added p99 in ns; delta is the percent change of that value.
  S6  pidfd_L4 and cgkill_L4 deadline_to_reaped at p99. Two rows per leg (old, new). The value
      is med_p99_ns; delta is the percent change.
The S4 and S6 delta definitions are an interpretation: their bounds are absolute, so a
percent-point difference of added percent would carry no meaning.
"""

import importlib.util
import os
import sys
from decimal import ROUND_HALF_EVEN, Decimal

HERE = os.path.dirname(os.path.abspath(__file__))
COMMITTED_RUN = os.path.join(HERE, "results", "20261028T034700Z")
CENT = Decimal("0.01")

TRANSFERS = "transfers"
NOT_TRANSFER = "does not transfer"
UNREADABLE = "unreadable (unstable)"


def load_r1():
    """Load r1.py from this directory so the eligibility rule is one definition."""
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location("aok_r1", os.path.join(HERE, "r1.py"))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


R1 = load_r1()
dec = R1.dec


def pct_change(old, new):
    """100*|new-old|/|old|, quantized; None when old is zero."""
    if old == 0:
        return None
    return (Decimal(100) * abs(new - old) / abs(old)).quantize(
        CENT, rounding=ROUND_HALF_EVEN
    )


def judge(rows, q, delta):
    """(verdict, tolerance) for a leg given the rows it compares and the delta."""
    if delta is None or not all(R1.is_eligible(r, q) for r in rows):
        return UNREADABLE, None
    tol = 2 * max(dec(r["spread_%s_pct" % q]) for r in rows)
    return (TRANSFERS if delta <= tol else NOT_TRANSFER), tol


def s3_leg(old_c0, old_c3, new_c0, new_c3, q):
    """(verdict, added_old, added_new, delta, tolerance) for one S3 leg and quantile."""
    rows = (old_c0, old_c3, new_c0, new_c3)
    if any(r is None for r in rows):
        return UNREADABLE, None, None, None, None
    a_old = R1.added_from_med(old_c3, old_c0, q)
    a_new = R1.added_from_med(new_c3, new_c0, q)
    delta = abs(a_new - a_old)
    verdict, tol = judge(rows, q, delta)
    return verdict, a_old, a_new, delta, tol


def added_ns(cfg, base, q):
    return dec(cfg["med_%s_ns" % q]) - dec(base["med_%s_ns" % q])


def s4_leg(old_tbl, new_tbl, base_cfg, cfg):
    q = "p99"
    rows = []
    for tbl in (old_tbl, new_tbl):
        for c in (base_cfg, cfg):
            rows.append(tbl.get(("broker", c, "roundtrip")))
    if any(r is None for r in rows):
        return UNREADABLE, None, None, None, None
    v_old = added_ns(rows[1], rows[0], q)
    v_new = added_ns(rows[3], rows[2], q)
    delta = pct_change(v_old, v_new)
    verdict, tol = judge(rows, q, delta)
    return verdict, v_old, v_new, delta, tol


def s6_leg(old_tbl, new_tbl, cfg):
    q = "p99"
    rows = [t.get(("deadline", cfg, "deadline_to_reaped")) for t in (old_tbl, new_tbl)]
    if any(r is None for r in rows):
        return UNREADABLE, None, None, None, None
    v_old = dec(rows[0]["med_p99_ns"])
    v_new = dec(rows[1]["med_p99_ns"])
    delta = pct_change(v_old, v_new)
    verdict, tol = judge(rows, q, delta)
    return verdict, v_old, v_new, delta, tol


def compare(old_dir, new_dir):
    """list of (leg, quantile, name, verdict, old, new, delta, tol)."""
    old = R1.load_summary(old_dir)
    new = R1.load_summary(new_dir)
    out = []
    for q in R1.QUANTILES:
        for bench in R1.S3_BENCHES:
            v, a, b, d, t = s3_leg(
                R1.micro(old, "C0", bench),
                R1.micro(old, "C3", bench),
                R1.micro(new, "C0", bench),
                R1.micro(new, "C3", bench),
                q,
            )
            out.append(("S3", q, bench, v, a, b, d, t))
    for base_cfg, cfg in (("direct", "broker_id"), ("egress_direct", "egress_broker")):
        v, a, b, d, t = s4_leg(old, new, base_cfg, cfg)
        out.append(("S4", "p99", "%s vs %s" % (cfg, base_cfg), v, a, b, d, t))
    for cfg in ("pidfd_L4", "cgkill_L4"):
        v, a, b, d, t = s6_leg(old, new, cfg)
        out.append(("S6", "p99", cfg, v, a, b, d, t))
    return out


def counts(results, leg, q):
    got = [r for r in results if r[0] == leg and r[1] == q]
    return (
        sum(1 for r in got if r[3] == TRANSFERS),
        sum(1 for r in got if r[3] == NOT_TRANSFER),
        [r[2] for r in got if r[3] == UNREADABLE],
    )


def show(v):
    return "-" if v is None else str(v)


def report(old_dir, new_dir):
    results = compare(old_dir, new_dir)
    print(
        "transfer: old %s, new %s"
        % (os.path.basename(old_dir), os.path.basename(new_dir))
    )
    print("leg quantile name: verdict (old, new, delta, tolerance)")
    for leg, q, name, v, a, b, d, t in results:
        print(
            "%s %s %s: %s (%s, %s, %s, %s)"
            % (leg, q, name, v, show(a), show(b), show(d), show(t))
        )
    for leg, q in (("S3", "p50"), ("S3", "p99"), ("S4", "p99"), ("S6", "p99")):
        n_ok, n_bad, unread = counts(results, leg, q)
        print(
            "%s %s: %d transfers, %d does not transfer, %d unreadable%s"
            % (
                leg,
                q,
                n_ok,
                n_bad,
                len(unread),
                " (%s)" % ", ".join(unread) if unread else "",
            )
        )
    return results


def synthetic_row(p50, s50, p99, s99):
    row = dict.fromkeys(R1.SUMMARIZE.FIELDS, "")
    row.update(
        med_p50_ns=p50,
        spread_p50_pct=s50,
        med_p99_ns=p99,
        spread_p99_pct=s99,
        stability=R1.SUMMARIZE.stability(float(Decimal(s50))),
    )
    return row


def self_test():
    fails = []
    results = report(COMMITTED_RUN, COMMITTED_RUN)

    def expect(leg, q, n_ok, n_bad, unread):
        got = counts(results, leg, q)
        if (got[0], got[1], sorted(got[2])) != (n_ok, n_bad, sorted(unread)):
            fails.append(
                "%s %s: got %r, want %r" % (leg, q, got, (n_ok, n_bad, unread))
            )

    expect("S3", "p50", 7, 0, [])
    expect("S3", "p99", 4, 0, ["openat_tmpfs", "openat_rootfs", "posix_spawn"])
    expect("S4", "p99", 0, 0, ["broker_id vs direct", "egress_broker vs egress_direct"])
    expect("S6", "p99", 0, 0, ["pidfd_L4", "cgkill_L4"])

    # Boundary: spreads 1.00 everywhere give tolerance 2.00. Added 10.00 against 12.00 is a
    # delta of exactly 2.00 and transfers; added 12.01 is one step over and does not.
    c0 = synthetic_row("1000.000", "1.00", "2000.000", "1.00")
    old3 = synthetic_row("1100.000", "1.00", "2200.000", "1.00")
    eq3 = synthetic_row("1120.000", "1.00", "2240.000", "1.00")
    over3 = synthetic_row("1120.100", "1.00", "2240.200", "1.00")
    for q in R1.QUANTILES:
        v, _, _, d, t = s3_leg(c0, old3, c0, eq3, q)
        if (v, d, t) != (TRANSFERS, Decimal("2.00"), Decimal("2.00")):
            fails.append("boundary %s: got %r" % (q, (v, d, t)))
        v, _, _, d, t = s3_leg(c0, old3, c0, over3, q)
        if v != NOT_TRANSFER or d <= t:
            fails.append("one step over %s: got %r" % (q, (v, d, t)))
    # Spread exactly 10.00 is ineligible even with a zero delta.
    edge = synthetic_row("1000.000", "10.00", "2000.000", "10.00")
    v = s3_leg(edge, old3, edge, old3, "p50")[0]
    if v != UNREADABLE:
        fails.append("spread 10.00: got %r, want %r" % (v, UNREADABLE))

    if fails:
        for f in fails:
            print("CHECK-FAIL: " + f)
        return 1
    print("CHECK-OK")
    return 0


def main(argv):
    args = argv[1:]
    if args == ["--self-test"]:
        return self_test()
    if len(args) == 2 and not args[0].startswith("-") and not args[1].startswith("-"):
        try:
            report(args[0], args[1])
        except (OSError, ValueError, KeyError) as e:
            print("transfer.py: %s" % (e,), file=sys.stderr)
            return 65
        return 0
    print(__doc__, file=sys.stderr)
    return 64


if __name__ == "__main__":
    sys.exit(main(sys.argv))
