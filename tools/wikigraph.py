#!/usr/bin/env python3
"""Build the wiki's knowledge graph from [[wikilinks]] and lint it.

Usage: python3 -I tools/wikigraph.py [wiki_dir]   writes <wiki_dir>/graph.json
       python3 -I tools/wikigraph.py --self-test
Exit 1 if any link points at a page that does not exist.
"""
import json
import re
import sys
import tempfile
from pathlib import Path

LINK = re.compile(r"\[\[([^\]|#]+)(?:#[^\]|]*)?(?:\|[^\]]*)?\]\]")
ROOT_PAGES = {"index", "log"}  # entry points, not expected to have inbound links


def build(wiki: Path):
    pages = {p.stem: p for p in wiki.rglob("*.md")}
    edges = set()
    for slug, path in pages.items():
        for target in LINK.findall(path.read_text(encoding="utf-8")):
            target = target.strip()
            if target != slug:
                edges.add((slug, target))
    broken = sorted((s, t) for s, t in edges if t not in pages)
    inbound = {t for _, t in edges}
    orphans = sorted(s for s in pages if s not in inbound and s not in ROOT_PAGES)
    nodes = [{"id": s, "group": p.parent.name} for s, p in sorted(pages.items())]
    links = [{"source": s, "target": t} for s, t in sorted(edges) if t in pages]
    return {"nodes": nodes, "links": links}, broken, orphans


def main(argv):
    if argv[1:] == ["--self-test"]:
        return self_test()
    wiki = Path(argv[1] if len(argv) > 1 else "wiki")
    graph, broken, orphans = build(wiki)
    (wiki / "graph.json").write_text(json.dumps(graph, indent=1) + "\n", encoding="utf-8")
    print(f"{len(graph['nodes'])} pages, {len(graph['links'])} links")
    for s, t in broken:
        print(f"BROKEN  {s} -> [[{t}]]")
    for s in orphans:
        print(f"ORPHAN  {s}")
    return 1 if broken else 0


def self_test():
    with tempfile.TemporaryDirectory() as d:
        w = Path(d)
        (w / "concepts").mkdir()
        (w / "index.md").write_text("[[a]] [[b|Bee]]")
        (w / "concepts" / "a.md").write_text("[[b#Section]] [[missing]] [[a]]")
        (w / "concepts" / "b.md").write_text("no links")
        (w / "concepts" / "c.md").write_text("[[a]]")
        graph, broken, orphans = build(w)
        assert broken == [("a", "missing")], broken
        assert orphans == ["c"], orphans
        assert {"source": "a", "target": "b"} in graph["links"]
        assert not any(l["source"] == l["target"] for l in graph["links"])
    print("self-test ok")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
