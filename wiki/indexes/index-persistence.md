# Index: Persistence

Storage, file systems and durability. Back to [[index]].

- [[atomic-rename|Atomic File Update via rename()]]: rename(old, new) is atomic with respect to crashes, so a file reads as the old or new version, never a mix; combined with write-to-temp and fsync it gives atomic whole-file replacement.
- [[crash-consistency|Crash Consistency]]: The problem of updating multiple on-disk structures when a crash can land between any two writes, leaving metadata inconsistent, space leaked, or garbage data visible.
- [[data-integrity|Data Integrity and Protection]]: Techniques to detect and recover from partial disk failures (latent sector errors and silent corruption) so data read equals data written.
- [[fast-file-system|Fast File System (FFS) and Locality]]: Berkeley redesign that keeps the file-system API but makes layout disk-aware: group related data and metadata near each other to avoid long seeks.
- [[file-and-directory-api|File and Directory API (POSIX)]]: The system-call interface for persistent storage: files are byte arrays named by an inode number, directories map names to inode numbers, and a single namespace is assembled by mounting.
- [[file-descriptor|File Descriptor and Open File Table]]: A small per-process integer indexing an array of pointers to entries in a system-wide open file table, each holding the current offset, flags, and a pointer to the inode.
- [[file-system-implementation|File System Implementation (vsfs)]]: The very simple file system: disk divided into superblock, inode and data bitmaps, inode table, and data blocks, plus the read/write paths and caching that operate on them.
- [[flash-ssd|Flash-based SSD and the FTL]]: NAND flash storage with no moving parts but asymmetric operations; a flash translation layer presents a normal block interface while hiding erase-before-program and wear-out.
- [[fsync-and-durability|fsync and Durability]]: write() only hands data to a buffering file system; fsync(fd) forces the file's dirty data to storage and returns when done, and is the application's lever on the durability/performance trade-off.
- [[hard-disk|Hard Disk Drive]]: A rotating-platter block device whose I/O time is seek plus rotational delay plus transfer, so sequential access is far cheaper than random access.
- [[inode|Inode]]: The on-disk structure holding a file's metadata and block pointers, addressed by inode number; directories supply the name-to-number mapping.
- [[io-devices-and-drivers|I/O Devices and Device Drivers]]: The OS talks to devices through a status/command/data register interface, hidden behind drivers so the rest of the kernel stays device-neutral.
- [[journaling|Journaling (Write-Ahead Logging)]]: Before updating structures in place, write a description of the update to a log; after a crash, replay committed transactions.
- [[log-structured-file-system|Log-Structured File System (LFS)]]: Buffer all updates, including metadata, into large in-memory segments and write them sequentially to free disk space; never overwrite in place.
- [[raid|RAID]]: Redundant Array of Independent Disks: combine many disks behind one block interface for capacity, performance, and reliability.
