#!/usr/bin/env python3
"""SAFE-01 gate: every `unsafe` site in the kernel and userland trees is ledgered and justified.

Usage: python3 -I tools/unsafe_check.py [--root DIR]   check the tree (default: this repo)
       python3 -I tools/unsafe_check.py --self-test
Exit 1 on any finding. Rules, with the ledger in docs/architecture/unsafe-ledger.md:
  a. an `unsafe` token or `allow(unsafe_code)` attribute in a file the ledger does not list
  b. an `unsafe` token with no `// SAFETY: <ID>` within the 3 preceding lines, ID listed for the file
  c. a ledger ID with no `// SAFETY: <ID>` comment in its listed file
  d. a `// SAFETY: <ID>` comment whose ID is not in the ledger
  e. a crate root without `#![deny(unsafe_code)]`
"""

import re
import sys
import tempfile
from pathlib import Path

LEDGER = "docs/architecture/unsafe-ledger.md"
UNSAFE = re.compile(r"\bunsafe\b")
ALLOW = re.compile(r"\b(?:allow|expect)\s*\([^)]*\bunsafe_code\b")
DENY = re.compile(r"#!\[\s*(?:deny|forbid)\s*\(\s*unsafe_code\s*\)\s*\]")
SAFETY = re.compile(r"//+\s*SAFETY:\s*([A-Z]-\d+)")
ROW_ID = {"Kernel": re.compile(r"K-\d\d"), "Userland": re.compile(r"U-\d\d")}
PATH = re.compile(r"[\w./-]+\.rs")


def blank(src: str) -> str:
    """`src` with comments, string and char literals replaced by spaces; newlines are kept."""
    out, i, n = [], 0, len(src)

    def skip(j):  # blank src[i:j], keeping newlines
        out.append(re.sub(r"[^\n]", " ", src[i:j]))
        return j

    while i < n:
        c = src[i]
        if src.startswith("//", i):
            j = src.find("\n", i)
            i = skip(n if j < 0 else j)
        elif src.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if src.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif src.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            i = skip(j)
        elif m := re.compile(r'b?r(#*)"').match(src, i):
            end = src.find('"' + m.group(1), m.end())
            i = skip(n if end < 0 else end + 1 + len(m.group(1)))
        elif c == '"':
            j = i + 1
            while j < n and src[j] != '"':
                j += 2 if src[j] == "\\" else 1
            i = skip(min(j + 1, n))
        elif c == "'":
            # char literal ('x', '\n', '\u{..}') vs lifetime ('a): a lifetime has no closing quote
            m = re.compile(r"'(?:\\.[^']*|[^\\'])'").match(src, i)
            if m:
                i = skip(m.end())
            else:
                out.append(c)
                i += 1
        else:
            out.append(c)
            i += 1
    return "".join(out)


def parse_ledger(path: Path) -> dict:
    """{id: repo-relative path} from the Kernel and Userland tables. ValueError on a bad row."""
    ids, section = {}, None
    for no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if line.startswith("## "):
            section = line[3:].strip()
            continue
        if section not in ROW_ID or not line.startswith("|"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if cells[0] == "ID" or set(cells[0]) <= set("-: "):
            continue
        if not ROW_ID[section].fullmatch(cells[0]):
            raise ValueError(
                f"{path.name}:{no}: bad ID {cells[0]!r} in section {section}"
            )
        loc = PATH.search(cells[1]) if len(cells) > 1 else None
        if not loc:
            raise ValueError(
                f"{path.name}:{no}: {cells[0]} has no .rs path in Location"
            )
        if cells[0] in ids:
            raise ValueError(f"{path.name}:{no}: duplicate ID {cells[0]}")
        ids[cells[0]] = loc.group(0)
    return ids


def rs_files(root: Path):
    yield from sorted(root.glob("kernel/src/**/*.rs"))
    yield from sorted(root.glob("userland/*/src/**/*.rs"))


def crate_roots(root: Path):
    yield root / "kernel/src/lib.rs"
    yield from sorted(root.glob("userland/*/src/lib.rs"))
    yield from sorted(root.glob("userland/tests/src/bin/*.rs"))


def scan(root: Path) -> dict:
    """{rel path: {"sites": [line, ...], "unsafe": [line, ...], "safety": [(line, ID), ...]}}"""
    found = {}
    for f in rs_files(root):
        raw = f.read_text(encoding="utf-8")
        clean = blank(raw)
        lines = clean.splitlines()
        unsafe = [i for i, l in enumerate(lines, 1) if UNSAFE.search(l)]
        allow = [i for i, l in enumerate(lines, 1) if ALLOW.search(l)]
        safety = [
            (i, m.group(1))
            for i, l in enumerate(raw.splitlines(), 1)
            for m in [SAFETY.search(l)]
            if m
        ]
        rel = f.relative_to(root).as_posix()
        found[rel] = {
            "sites": sorted(set(unsafe + allow)),
            "unsafe": unsafe,
            "safety": safety,
        }
    return found


def check(root: Path) -> list:
    out = []
    try:
        ledger = parse_ledger(root / LEDGER)
    except (OSError, ValueError) as e:
        return [f"{LEDGER}: {e}"]
    found = scan(root)
    for rel, info in found.items():
        mine = {i for i, p in ledger.items() if p == rel}
        for line in info["sites"] if not mine else []:
            out.append(
                f"{rel}:{line}: unsafe site in a file the ledger does not list (a)"
            )
        for line in info["unsafe"] if mine else []:
            near = {i for n, i in info["safety"] if line - 3 <= n <= line}
            if not near & mine:
                out.append(
                    f"{rel}:{line}: unsafe without a `// SAFETY: <ID>` comment (b)"
                )
        for n, i in info["safety"]:
            if i not in ledger:
                out.append(f"{rel}:{n}: SAFETY ID {i} is not in the ledger (d)")
    for i, rel in sorted(ledger.items()):
        if not any(i == sid for _, sid in found.get(rel, {}).get("safety", [])):
            out.append(f"{rel}:1: ledger ID {i} has no `// SAFETY: {i}` comment (c)")
    for f in crate_roots(root):
        if not DENY.search(blank(f.read_text(encoding="utf-8"))):
            out.append(
                f"{f.relative_to(root).as_posix()}:1: crate root lacks #![deny(unsafe_code)] (e)"
            )
    return out


def main(argv):
    if argv[1:] == ["--self-test"]:
        return self_test()
    root = Path(__file__).resolve().parent.parent
    if len(argv) == 3 and argv[1] == "--root":
        root = Path(argv[2])
    elif len(argv) != 1:
        print(__doc__)
        return 2
    findings = check(root)
    for f in findings:
        print(f)
    if findings:
        return 1
    print("unsafe_check: ok")
    return 0


def self_test():
    ledger = (
        "## Kernel\n| ID | Location | Why | Inv | Retire |\n|---|---|---|---|---|\n"
        "{krows}\n## Userland\n| ID | Location | Why | Inv | Retire |\n|---|---|---|---|---|\n"
        "| U-01 | `userland/libsys/src/raw.rs` | asm | held | never |\n## FPU\nprose\n"
    )
    good_raw = '#![allow(unsafe_code)]\n// SAFETY: U-01 reason\nfn f() { unsafe { core::arch::asm!("nop") } }\n'
    base = {
        "kernel/src/lib.rs": "#![deny(unsafe_code)]\n",
        "userland/libsys/src/lib.rs": "#![deny(unsafe_code)]\nmod raw;\n",
        "userland/libsys/src/raw.rs": good_raw,
        "userland/tests/src/bin/t.rs": "#![deny(unsafe_code)]\n",
    }

    def run(files=None, krows="", drop=()):
        with tempfile.TemporaryDirectory() as d:
            r = Path(d)
            tree = {**base, **(files or {})}
            tree[LEDGER] = ledger.format(krows=krows)
            for rel, text in tree.items():
                if rel in drop:
                    continue
                (r / rel).parent.mkdir(parents=True, exist_ok=True)
                (r / rel).write_text(text, encoding="utf-8")
            return check(r)

    def has(findings, tag, path=None):
        return any(
            f"({tag})" in f and (path is None or f.startswith(path)) for f in findings
        )

    assert run() == [], run()
    # a: unlisted file, both token and attribute
    assert has(
        run({"kernel/src/x.rs": "fn f() { unsafe {} }\n"}), "a", "kernel/src/x.rs:1"
    )
    assert has(
        run({"kernel/src/x.rs": "#![allow(unsafe_code)]\n"}), "a", "kernel/src/x.rs:1"
    )
    # b: listed file, no SAFETY comment in range (comment 4 lines up is too far)
    far = "// SAFETY: U-01 x\n\n\n\nfn f() { unsafe {} }\n"
    assert has(
        run({"userland/libsys/src/raw.rs": far}), "b", "userland/libsys/src/raw.rs:5"
    )
    # c: ledger row whose file has no matching comment
    assert has(
        run(
            krows="| K-01 | `kernel/src/fpu.rs` | a | b | c |",
            files={"kernel/src/fpu.rs": ""},
        ),
        "c",
    )
    # d: SAFETY ID not in the ledger
    assert has(
        run({"kernel/src/y.rs": "// SAFETY: K-09 x\n"}), "d", "kernel/src/y.rs:1"
    )
    # e: crate root missing deny, for kernel and a ring-3 bin
    assert has(run({"kernel/src/lib.rs": "fn f() {}\n"}), "e", "kernel/src/lib.rs")
    assert has(
        run({"userland/tests/src/bin/t.rs": "fn main() {}\n"}),
        "e",
        "userland/tests/src/bin/t.rs",
    )
    # deny alone is not a site; unsafe in comments, strings, raw strings and char/lifetime noise is ignored
    noise = (
        "#![deny(unsafe_code)]\n// unsafe\n/* unsafe /* nested unsafe */ */\n"
        'const S: &str = "unsafe \\" unsafe";\nconst R: &str = r#"unsafe"#;\n'
        "const C: char = '\\''; fn g<'a>(x: &'a str) -> &'a str { x }\n"
    )
    assert run({"kernel/src/lib.rs": noise}) == [], run({"kernel/src/lib.rs": noise})
    # a malformed ledger row is an error, not a pass
    bad = run(krows="| X-01 | `kernel/src/a.rs` | a | b | c |")
    assert bad and bad[0].startswith(LEDGER), bad
    print("unsafe_check: self-test ok")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
