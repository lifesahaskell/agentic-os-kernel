"""Gate for docs/architecture/research/linux-baseline.md.

Usage: python3 -I check_report.py <report.md> <run-dir>

Prints REPORT-OK, or the first failing assertion and exits 1.
"""

import os
import re
import sys

HEADINGS = [
    "## 1. Verdict",
    "## 2. Host",
    "## 3. Method",
    "## 4. Microbenchmarks (S3)",
    "## 5. Broker round trip (S4)",
    "## 6. Deadline enforcement (S6 wall-time leg)",
    "## 7. cgroup probe and legs",
    "## 8. Not measured",
    "## 9. Verified and assumed",
    "## 10. Caveats",
    "## 11. Proposed revisions to S3, S4 and S6",
    "## Sources",
]
STATUS = "proposed, awaiting user acceptance"


def fail(msg):
    print("REPORT-FAIL: " + msg)
    sys.exit(1)


def strip_code(text):
    text = re.sub(r"^```.*?^```[ \t]*$", "", text, flags=re.S | re.M)
    return re.sub(r"`[^`\n]*`", "", text)


def sections(text):
    """Map heading line to its body (up to the next level-2 heading)."""
    out = {}
    cur = None
    in_fence = False
    for line in text.split("\n"):
        if line.startswith("```"):
            in_fence = not in_fence
        if not in_fence and line.startswith("## "):
            cur = line.rstrip()
            out[cur] = []
            continue
        if cur is not None:
            out[cur].append(line)
    return {k: "\n".join(v) for k, v in out.items()}


def main():
    if len(sys.argv) != 3:
        print("usage: check_report.py <report.md> <run-dir>")
        sys.exit(2)
    report, run = sys.argv[1], sys.argv[2]
    text = open(report, encoding="utf-8").read()
    prose = strip_code(text)

    # (1) voice
    if "!" in prose:
        fail("(1) exclamation mark in prose")
    for ch in prose:
        cp = ord(ch)
        if 0x1F000 <= cp <= 0x1FAFF or 0x2600 <= cp <= 0x27BF:
            fail("(1) emoji or symbol U+%04X in prose" % cp)
    low = prose.lower()
    for phrase in ("it seems", "perhaps", "arguably"):
        if phrase in low:
            fail("(1) hedging phrase '%s' in prose" % phrase)

    # (2) outline, in order
    pos = -1
    lines = text.split("\n")
    for h in HEADINGS:
        idx = next(
            (i for i, ln in enumerate(lines) if i > pos and ln.rstrip() == h), None
        )
        if idx is None:
            fail("(2) heading missing or out of order: " + h)
        pos = idx

    secs = sections(text)

    # (3) section 8
    s8 = strip_code(secs["## 8. Not measured"])
    for need in ("NOT MEASURED", "BPF-LSM", "BPF_LSM_CGROUP", "S7", "S8"):
        if need not in s8:
            fail("(3) section 8 prose lacks " + need)

    # (4) section 9
    s9 = secs["## 9. Verified and assumed"]
    for need in ("VERIFIED", "ASSUMED"):
        if need not in s9:
            fail("(4) section 9 lacks " + need)

    # (5) section 11 table
    s11 = secs["## 11. Proposed revisions to S3, S4 and S6"]
    data = [ln for ln in s11.split("\n") if ln.startswith("|")][2:]
    if not data:
        fail("(5) section 11 has no table rows")
    ids = set()
    for ln in data:
        if STATUS not in ln:
            fail("(5) section 11 row lacks status: " + ln[:60])
        ids.add(ln.split("|")[1].strip().split()[0][:2])
    for need in ("S3", "S4", "S6"):
        if need not in ids:
            fail("(5) section 11 has no %s row" % need)

    # (6) cited result paths
    cited = re.findall(r"tools/linux-baseline/results/[^\s)\]]+", text)
    if not cited:
        fail("(6) no results path cited")
    run_norm = os.path.normpath(run)
    names_run = False
    for c in cited:
        if not os.path.exists(c):
            fail("(6) cited path does not exist: " + c)
        if os.path.normpath(c).startswith(run_norm):
            names_run = True
    if not names_run:
        fail("(6) no cited path names the run directory")

    # (7) numeric provenance in the tables of sections 4 to 7
    blob = ""
    for name in sorted(os.listdir(run)):
        p = os.path.join(run, name)
        if os.path.isfile(p):
            blob += open(p, encoding="utf-8", errors="replace").read() + "\n"
    for h in HEADINGS[3:7]:
        for ln in secs[h].split("\n"):
            if not ln.startswith("|"):
                continue
            for tok in re.findall(r"\d+\.\d+|\d{3,}", ln):
                if tok not in blob:
                    fail("(7) number %s in '%s' is not in the run files" % (tok, h))

    print("REPORT-OK")


main()
