# Index: Distribution

Networking and distributed systems. Back to [[index]].

- [[afs|AFS (Andrew File System)]]: A distributed file system built for scale, using whole-file caching on client disks and server callbacks instead of polling.
- [[cache-consistency|Cache consistency]]: When clients cache remote data, writers' updates must become visible and readers must not use stale copies; protocols differ in when they guarantee this.
- [[distributed-systems-communication|Distributed systems communication]]: Machines cooperate only by exchanging messages over a network that can lose, corrupt, or reorder packets.
- [[idempotency|Idempotent operations and retry safety]]: An operation that gives the same result when applied repeatedly can be retried blindly after a lost reply, which removes the need for server-side memory of requests.
- [[nfs|NFS (v2), a stateless network file system]]: A client/server file protocol designed so the server keeps no per-client state and therefore recovers from a crash by simply restarting.
- [[rpc|Remote procedure call]]: Makes a remote function call look like a local one by generating stubs that marshal arguments and a run-time library that moves the bytes.
