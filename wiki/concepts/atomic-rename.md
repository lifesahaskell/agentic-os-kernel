---
title: Atomic File Update via rename()
type: concept
sources: [ostep]
tags: [persistence]
---

# Atomic File Update via rename()

rename(old, new) is atomic with respect to crashes, so a file reads as the old or new version, never a mix; combined with write-to-temp and fsync it gives atomic whole-file replacement.

## Mechanisms

- rename(char *old, char *new) is the call mv uses; atomicity is "usually" implemented, per the text (ostep p. 513)
- Pattern: open("foo.txt.tmp", O_WRONLY|O_CREAT|O_TRUNC), write, fsync(fd), close, rename to foo.txt (ostep p. 513)
- The swap atomically puts the new file in place and removes the old version (ostep p. 513)
- FFS introduced the atomic rename as a usability feature (ostep p. 558)
- The temp file must be fully durable before the rename, else a crash can expose an empty or partial new file (ostep p. 513)
- Study of application crash behavior finds applications expect stronger guarantees than file systems give (ostep p. 578)

## Trade-offs

- Whole-file rewrite is simple and safe but costs a full copy per update, so it suits small or infrequently changed state.

## Agentic OS relevance

- (interpretation) The default safe way for an agent to update a state or memory file: temp file in the same directory, fsync, rename, then fsync the directory.

## Related

- [[fsync-and-durability]]
- [[file-and-directory-api]]
- [[crash-consistency]]
- [[fast-file-system]]
