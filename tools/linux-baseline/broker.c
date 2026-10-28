/* S4 broker round trip on Linux (L8). Everything here runs unrestricted (config C0).
 *
 * Variants (one fresh process each):
 *   direct         table lookup in-process
 *   broker_noid    SOCK_SEQPACKET request to a broker child, table lookup, reply
 *   broker_id      as broker_noid, but the broker first fetches the caller's identity with
 *                  SO_PEERCRED, SO_PEERPIDFD and PIDFD_GET_INFO, and keys the lookup on it
 *   egress_direct  socket + SO_LINGER{1,0} + connect + close to a local TCP listener
 *   egress_broker  the broker performs that connect and passes the fd back with SCM_RIGHTS
 */
#include <arpa/inet.h>
#include <errno.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/prctl.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <sys/un.h>
#include <sys/wait.h>
#include <unistd.h>

#include "common.h"
#include "uapi_compat.h"

#define SLOTS 4096
#define ENTRIES 2048
#define MSG 64

enum { OP_LOOKUP = 0, OP_EGRESS = 1, OP_IDENT = 2 };

struct msg {
	uint32_t op;
	uint32_t pad;
	uint64_t key;	/* lookup: key; egress: TCP port */
	uint64_t a, b, c;	/* replies: value / ident fields */
	unsigned char fill[MSG - 40];
};

_Static_assert(sizeof(struct msg) == MSG, "msg size");

static uint64_t mix(uint64_t x)
{
	x += 0x9e3779b97f4a7c15ULL;
	x = (x ^ (x >> 30)) * 0xbf58476d1ce4e5b9ULL;
	x = (x ^ (x >> 27)) * 0x94d049bb133111ebULL;
	return x ^ (x >> 31);
}

static uint64_t id_hash(uint32_t uid, uint64_t cgid, uint64_t k)
{
	return mix(mix(((uint64_t)uid << 32) ^ mix(cgid)) ^ k);
}

struct slot {
	uint64_t key, val;
	int used;
};

static struct slot table[SLOTS];

static void table_put(uint64_t h, uint64_t v)
{
	size_t i = h & (SLOTS - 1);
	while (table[i].used)
		i = (i + 1) & (SLOTS - 1);
	table[i].key = h;
	table[i].val = v;
	table[i].used = 1;
}

static __attribute__((noinline)) uint64_t table_get(uint64_t h, int *found)
{
	size_t i = h & (SLOTS - 1);
	while (table[i].used) {
		if (table[i].key == h) {
			*found = 1;
			return table[i].val;
		}
		i = (i + 1) & (SLOTS - 1);
	}
	*found = 0;
	return 0;
}

static uint64_t expect_val(uint64_t k)
{
	return mix(k + 1);
}

/* Identity of the caller, as the broker will compute it. */
struct ident {
	uint32_t uid;
	uint64_t cgid;
};

static int g_have_pidfd_info;	/* broker_id can use SO_PEERPIDFD + PIDFD_GET_INFO */
static char g_peerpidfd_status[32] = "ok";
static char g_getinfo_status[32] = "ok";

/* Probe on a socketpair: peer is this process, so the pidfd names us. */
static void probe_pidfd(void)
{
	int sv[2];
	if (socketpair(AF_UNIX, SOCK_SEQPACKET | SOCK_CLOEXEC, 0, sv) != 0)
		die("socketpair");
	int pidfd = -1;
	socklen_t l = sizeof pidfd;
	if (getsockopt(sv[0], SOL_SOCKET, AOK_SO_PEERPIDFD, &pidfd, &l) != 0) {
		snprintf(g_peerpidfd_status, sizeof g_peerpidfd_status, "unsupported:%d", errno);
		snprintf(g_getinfo_status, sizeof g_getinfo_status, "unsupported:%d", errno);
		close(sv[0]);
		close(sv[1]);
		return;
	}
	struct aok_pidfd_info info;
	memset(&info, 0, sizeof info);
	info.mask = AOK_PIDFD_INFO_CGROUPID;
	errno = 0;
	if (ioctl(pidfd, AOK_PIDFD_GET_INFO, &info) != 0 || info.pid != (uint32_t)getpid() ||
	    !(info.mask & AOK_PIDFD_INFO_CGROUPID))
		snprintf(g_getinfo_status, sizeof g_getinfo_status, "unsupported:%d", errno);
	else
		g_have_pidfd_info = 1;
	close(pidfd);
	close(sv[0]);
	close(sv[1]);
}

/* cgroup id the way the selfcheck computes it: st_ino of the caller's cgroup directory. */
static uint64_t own_cgroup_ino(void)
{
	char buf[1024], path[1100];
	int fd = open("/proc/self/cgroup", O_RDONLY | O_CLOEXEC);
	if (fd < 0)
		return 0;
	ssize_t r = read(fd, buf, sizeof buf - 1);
	close(fd);
	if (r <= 0)
		return 0;
	buf[r] = 0;
	char *p = strstr(buf, "0::");
	if (!p)
		return 0;
	p += 3;
	p[strcspn(p, "\n")] = 0;
	snprintf(path, sizeof path, "/sys/fs/cgroup%s", p);
	struct stat st;
	return stat(path, &st) == 0 ? (uint64_t)st.st_ino : 0;
}

/* Broker side: identity of the peer of fd. Returns 0 on success. */
static int broker_ident(int fd, struct ident *id, uint32_t *peercred_pid, uint32_t *info_pid)
{
	struct ucred uc;
	socklen_t l = sizeof uc;
	if (getsockopt(fd, SOL_SOCKET, SO_PEERCRED, &uc, &l) != 0)
		return -1;
	id->uid = uc.uid;
	id->cgid = 0;
	*peercred_pid = uc.pid;
	*info_pid = 0;
	if (!g_have_pidfd_info)
		return 0;
	int pidfd = -1;
	l = sizeof pidfd;
	if (getsockopt(fd, SOL_SOCKET, AOK_SO_PEERPIDFD, &pidfd, &l) != 0)
		return -1;
	struct aok_pidfd_info info;
	memset(&info, 0, sizeof info);
	info.mask = AOK_PIDFD_INFO_CGROUPID;
	int rc = ioctl(pidfd, AOK_PIDFD_GET_INFO, &info);
	close(pidfd);
	if (rc != 0)
		return -1;
	id->cgid = info.cgroupid;
	*info_pid = info.pid;
	return 0;
}

static void send_fd(int sock, int fd)
{
	struct msg m;
	memset(&m, 0, sizeof m);
	char cbuf[CMSG_SPACE(sizeof(int))];
	memset(cbuf, 0, sizeof cbuf);
	struct iovec iov = { &m, sizeof m };
	struct msghdr mh = { .msg_iov = &iov, .msg_iovlen = 1, .msg_control = cbuf,
			     .msg_controllen = sizeof cbuf };
	struct cmsghdr *c = CMSG_FIRSTHDR(&mh);
	c->cmsg_level = SOL_SOCKET;
	c->cmsg_type = SCM_RIGHTS;
	c->cmsg_len = CMSG_LEN(sizeof(int));
	memcpy(CMSG_DATA(c), &fd, sizeof fd);
	sendmsg(sock, &mh, 0);
}

static void broker_serve(int lfd, int mode_id)
{
	prctl(PR_SET_PDEATHSIG, SIGKILL);
	int fd = accept4(lfd, NULL, NULL, SOCK_CLOEXEC);
	if (fd < 0)
		_exit(1);
	struct msg m;
	while (recv(fd, &m, sizeof m, 0) == (ssize_t)sizeof m) {
		struct msg r;
		memset(&r, 0, sizeof r);
		if (m.op == OP_LOOKUP) {
			int found;
			uint64_t h = m.key;
			if (mode_id) {
				struct ident id;
				uint32_t a, b;
				if (broker_ident(fd, &id, &a, &b) != 0) {
					r.a = 0;
					send(fd, &r, sizeof r, 0);
					continue;
				}
				h = id_hash(id.uid, id.cgid, m.key);
			} else {
				h = mix(m.key);
			}
			r.a = table_get(h, &found);
			if (!found)
				r.a = 0;
			send(fd, &r, sizeof r, 0);
		} else if (m.op == OP_IDENT) {
			struct ident id = { 0, 0 };
			uint32_t pc = 0, ip = 0;
			int rc = broker_ident(fd, &id, &pc, &ip);
			r.a = rc == 0;
			r.b = ((uint64_t)pc << 32) | ip;
			r.c = id.cgid;
			r.key = id.uid;
			send(fd, &r, sizeof r, 0);
		} else if (m.op == OP_EGRESS) {
			int s = socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC, 0);
			struct linger lg = { 1, 0 };
			struct sockaddr_in a;
			memset(&a, 0, sizeof a);
			a.sin_family = AF_INET;
			a.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
			a.sin_port = htons((uint16_t)m.key);
			if (s < 0 || setsockopt(s, SOL_SOCKET, SO_LINGER, &lg, sizeof lg) != 0 ||
			    connect(s, (struct sockaddr *)&a, sizeof a) != 0) {
				r.a = 1;
				send(fd, &r, sizeof r, 0);
			} else {
				send_fd(fd, s);
			}
			if (s >= 0)
				close(s);
		}
	}
	_exit(0);
}

static void drain(int lfd)
{
	int c;
	while ((c = accept4(lfd, NULL, NULL, SOCK_NONBLOCK | SOCK_CLOEXEC)) >= 0)
		close(c);
}

struct bench_state {
	int variant;	/* 0 direct, 1 noid, 2 id, 3 egress_direct, 4 egress_broker */
	int bfd;	/* connected broker socket */
	int tcp_lfd;
	uint16_t tcp_port;
	struct ident self_id;
	volatile uint64_t sink;
};

static long run_loop(struct bench_state *s, double *samples, size_t n)
{
	long errors = 0;
	for (size_t i = 0; i < n; i++) {
		uint64_t k = i & (ENTRIES - 1);
		struct msg q, r;
		uint64_t t0, t1;
		switch (s->variant) {
		case 0: {
			int found;
			uint64_t h = mix(k);
			t0 = now_ns(RAW_CLOCK);
			uint64_t v = table_get(h, &found);
			t1 = now_ns(RAW_CLOCK);
			s->sink = v;
			if (!found || v != expect_val(k))
				errors++;
			break;
		}
		case 1:
		case 2:
			memset(&q, 0, sizeof q);
			q.op = OP_LOOKUP;
			q.key = k;
			t0 = now_ns(RAW_CLOCK);
			if (send(s->bfd, &q, sizeof q, 0) != (ssize_t)sizeof q ||
			    recv(s->bfd, &r, sizeof r, 0) != (ssize_t)sizeof r) {
				errors++;
				t1 = now_ns(RAW_CLOCK);
				break;
			}
			t1 = now_ns(RAW_CLOCK);
			if (r.a != expect_val(k))
				errors++;
			break;
		case 3: {
			t0 = now_ns(RAW_CLOCK);
			int sk = socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC, 0);
			struct linger lg = { 1, 0 };
			struct sockaddr_in a;
			memset(&a, 0, sizeof a);
			a.sin_family = AF_INET;
			a.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
			a.sin_port = htons(s->tcp_port);
			int bad = sk < 0 || setsockopt(sk, SOL_SOCKET, SO_LINGER, &lg, sizeof lg) != 0 ||
				  connect(sk, (struct sockaddr *)&a, sizeof a) != 0;
			if (sk >= 0)
				close(sk);
			t1 = now_ns(RAW_CLOCK);
			errors += bad;
			drain(s->tcp_lfd);
			break;
		}
		default: {
			memset(&q, 0, sizeof q);
			q.op = OP_EGRESS;
			q.key = s->tcp_port;
			char cbuf[CMSG_SPACE(sizeof(int))];
			struct iovec iov = { &r, sizeof r };
			struct msghdr mh = { .msg_iov = &iov, .msg_iovlen = 1, .msg_control = cbuf,
					     .msg_controllen = sizeof cbuf };
			int got = -1;
			t0 = now_ns(RAW_CLOCK);
			if (send(s->bfd, &q, sizeof q, 0) != (ssize_t)sizeof q ||
			    recvmsg(s->bfd, &mh, MSG_CMSG_CLOEXEC) != (ssize_t)sizeof r) {
				errors++;
			} else {
				struct cmsghdr *c = CMSG_FIRSTHDR(&mh);
				if (c && c->cmsg_level == SOL_SOCKET && c->cmsg_type == SCM_RIGHTS)
					memcpy(&got, CMSG_DATA(c), sizeof got);
				if (got < 0)
					errors++;
				else
					close(got);
			}
			t1 = now_ns(RAW_CLOCK);
			drain(s->tcp_lfd);
			break;
		}
		}
		samples[i] = (double)(t1 - t0);
	}
	return errors;
}

static const struct {
	const char *name;
	size_t n;
} variants[] = {
	{ "direct", 200000 },	  { "broker_noid", 100000 },	{ "broker_id", 100000 },
	{ "egress_direct", 20000 }, { "egress_broker", 20000 },
};

int main(int argc, char **argv)
{
	const char *variant = NULL, *tmpfs_dir = NULL, *out = NULL, *selfcheck = NULL;
	int round = 0, smoke = 0;
	for (int i = 1; i < argc; i++) {
		if (!strcmp(argv[i], "--smoke"))
			smoke = 1;
		else if (i + 1 < argc && !strcmp(argv[i], "--variant"))
			variant = argv[++i];
		else if (i + 1 < argc && !strcmp(argv[i], "--round"))
			round = atoi(argv[++i]);
		else if (i + 1 < argc && !strcmp(argv[i], "--tmpfs-dir"))
			tmpfs_dir = argv[++i];
		else if (i + 1 < argc && !strcmp(argv[i], "--out"))
			out = argv[++i];
		else if (i + 1 < argc && !strcmp(argv[i], "--selfcheck"))
			selfcheck = argv[++i];
		else {
			fprintf(stderr, "broker: bad argument %s\n", argv[i]);
			return 64;
		}
	}
	if (!variant || !tmpfs_dir || !out || !selfcheck || round < 1) {
		fprintf(stderr, "usage: broker --variant V --round R --tmpfs-dir D --out F "
				"--selfcheck F [--smoke]\n");
		return 64;
	}
	int vi = -1;
	for (int i = 0; i < 5; i++)
		if (!strcmp(variant, variants[i].name))
			vi = i;
	if (vi < 0) {
		fprintf(stderr, "broker: unknown variant %s\n", variant);
		return 64;
	}

	FILE *csv = fopen(out, "a");
	FILE *sc = fopen(selfcheck, "a");
	if (!csv || !sc)
		die("open out/selfcheck");

	size_t n = scale_n(variants[vi].n, smoke);
	size_t w = scale_warm(variants[vi].n / 10, smoke);
	double *samples = samples_alloc(n > w ? n : w);

	struct bench_state st = { .variant = vi, .bfd = -1 };
	probe_pidfd();
	st.self_id.uid = (uint32_t)getuid();
	st.self_id.cgid = (vi == 2 && g_have_pidfd_info) ? own_cgroup_ino() : 0;

	/* Identical table in caller and broker (built before fork). broker_id keys on identity. */
	for (uint64_t k = 0; k < ENTRIES; k++)
		table_put(vi == 2 ? id_hash(st.self_id.uid, st.self_id.cgid, k) : mix(k),
			  expect_val(k));

	/* TCP listener for the egress variants. */
	st.tcp_lfd = socket(AF_INET, SOCK_STREAM | SOCK_NONBLOCK | SOCK_CLOEXEC, 0);
	struct sockaddr_in a;
	memset(&a, 0, sizeof a);
	a.sin_family = AF_INET;
	a.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
	socklen_t al = sizeof a;
	if (st.tcp_lfd < 0 || bind(st.tcp_lfd, (struct sockaddr *)&a, sizeof a) != 0 ||
	    listen(st.tcp_lfd, 1024) != 0 || getsockname(st.tcp_lfd, (struct sockaddr *)&a, &al) != 0)
		die("tcp listener");
	st.tcp_port = ntohs(a.sin_port);

	long errors = 0;
	char idline[256] = "";
	pid_t bpid = -1;
	if (vi == 1 || vi == 2 || vi == 4) {
		char path[300];
		snprintf(path, sizeof path, "%s/broker.sock", tmpfs_dir);
		unlink(path);
		struct sockaddr_un ua;
		memset(&ua, 0, sizeof ua);
		ua.sun_family = AF_UNIX;
		if (strlen(path) >= sizeof ua.sun_path)
			die("socket path too long");
		strcpy(ua.sun_path, path);
		int lfd = socket(AF_UNIX, SOCK_SEQPACKET | SOCK_CLOEXEC, 0);
		if (lfd < 0 || bind(lfd, (struct sockaddr *)&ua, sizeof ua) != 0 || listen(lfd, 4) != 0)
			die("broker listener");
		bpid = fork();
		if (bpid < 0)
			die("fork broker");
		if (bpid == 0)
			broker_serve(lfd, vi == 2);
		close(lfd);
		st.bfd = socket(AF_UNIX, SOCK_SEQPACKET | SOCK_CLOEXEC, 0);
		if (st.bfd < 0 || connect(st.bfd, (struct sockaddr *)&ua, sizeof ua) != 0)
			die("connect broker");
		unlink(path);

		if (vi == 2) {
			/* Selfcheck: the broker reports the identity it derived; compare with ours. */
			struct msg q, r;
			memset(&q, 0, sizeof q);
			q.op = OP_IDENT;
			if (send(st.bfd, &q, sizeof q, 0) != (ssize_t)sizeof q ||
			    recv(st.bfd, &r, sizeof r, 0) != (ssize_t)sizeof r || !r.a) {
				errors++;
			} else {
				uint32_t pc = (uint32_t)(r.b >> 32), ip = (uint32_t)(r.b & 0xffffffffu);
				int pid_match = pc == (uint32_t)getpid() && (!g_have_pidfd_info || ip == pc);
				int cg_match = g_have_pidfd_info && r.c == own_cgroup_ino() && r.c != 0;
				if (!pid_match || (g_have_pidfd_info && !cg_match) ||
				    (uint32_t)r.key != st.self_id.uid)
					errors++;
				snprintf(idline, sizeof idline,
					 "so_peercred=ok so_peerpidfd=%s pidfd_get_info=%s pid_match=%s "
					 "cgroupid_match=%s",
					 g_peerpidfd_status, g_getinfo_status, pid_match ? "yes" : "no",
					 g_have_pidfd_info ? (cg_match ? "yes" : "no") : "na");
			}
		}
	}

	errors += run_loop(&st, samples, w);
	long e = run_loop(&st, samples, n);
	errors += e;
	emit_row(csv, "broker", round, variant, "roundtrip", n, w, samples, e);
	fclose(csv);

	if (vi == 2)
		fprintf(sc, "%s round=%d %s errors=%ld result=%s\n", variant, round, idline, errors,
			errors ? "FAIL" : "ok");
	else
		fprintf(sc, "%s round=%d errors=%ld result=%s\n", variant, round, errors,
			errors ? "FAIL" : "ok");
	fclose(sc);

	if (st.bfd >= 0)
		close(st.bfd);
	if (bpid > 0) {
		kill(bpid, SIGKILL);
		waitpid(bpid, NULL, 0);
	}
	return errors ? 2 : 0;
}
