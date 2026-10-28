/* Shared helpers for the Linux baseline harness. Header-only, glibc only. */
#ifndef AOK_COMMON_H
#define AOK_COMMON_H

#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>

#define RAW_CLOCK CLOCK_MONOTONIC_RAW

static inline uint64_t now_ns(clockid_t clk)
{
	struct timespec ts;
	clock_gettime(clk, &ts);
	return (uint64_t)ts.tv_sec * 1000000000ULL + (uint64_t)ts.tv_nsec;
}

static inline void die(const char *what)
{
	fprintf(stderr, "FATAL: %s: %s\n", what, strerror(errno));
	exit(2);
}

/* Smoke scaling: divide by 50, round up, minimum 20 samples. */
static inline size_t scale_n(size_t n, int smoke)
{
	if (!smoke)
		return n;
	size_t v = (n + 49) / 50;
	return v < 20 ? 20 : v;
}

/* Smoke scaling for warm-up: divide by 50, round up, minimum 5. */
static inline size_t scale_warm(size_t w, int smoke)
{
	if (!smoke)
		return w;
	size_t v = (w + 49) / 50;
	return v < 5 ? 5 : v;
}

/* Sample buffer: mmap, then pre-touch so no page fault lands inside a timed region. */
static inline double *samples_alloc(size_t n)
{
	size_t bytes = n * sizeof(double);
	double *p = mmap(NULL, bytes, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
	if (p == MAP_FAILED)
		die("mmap samples");
	memset(p, 0, bytes);
	return p;
}

struct stats {
	double min, p50, p90, p99, p999, max, mean;
};

static inline int cmp_double(const void *a, const void *b)
{
	double x = *(const double *)a, y = *(const double *)b;
	return (x > y) - (x < y);
}

/* Nearest rank: q-quantile = x[ceil(q*n)-1], q given in per-mille to stay in integers. */
static inline double nr_quantile(const double *sorted, size_t n, unsigned permille)
{
	size_t k = ((size_t)permille * n + 999) / 1000;
	if (k < 1)
		k = 1;
	return sorted[k - 1];
}

/* Sorts the samples in place. */
static inline struct stats compute_stats(double *s, size_t n)
{
	struct stats st;
	double sum = 0;
	qsort(s, n, sizeof(double), cmp_double);
	for (size_t i = 0; i < n; i++)
		sum += s[i];
	st.min = s[0];
	st.max = s[n - 1];
	st.p50 = nr_quantile(s, n, 500);
	st.p90 = nr_quantile(s, n, 900);
	st.p99 = nr_quantile(s, n, 990);
	st.p999 = nr_quantile(s, n, 999);
	st.mean = sum / (double)n;
	return st;
}

#define CSV_HEADER                                                                                 \
	"suite,round,config,bench,n,warmup,min_ns,p50_ns,p90_ns,p99_ns,p999_ns,max_ns,mean_ns,errors\n"

/* Appends one CSV row; writes the header first when the file is empty. */
static inline void emit_row(FILE *f, const char *suite, int round, const char *config,
			    const char *bench, size_t n, size_t warmup, double *samples, long errors)
{
	struct stats st = compute_stats(samples, n);
	fseek(f, 0, SEEK_END);
	if (ftell(f) == 0)
		fputs(CSV_HEADER, f);
	fprintf(f, "%s,%d,%s,%s,%zu,%zu,%.3f,%.3f,%.3f,%.3f,%.3f,%.3f,%.3f,%ld\n", suite, round,
		config, bench, n, warmup, st.min, st.p50, st.p90, st.p99, st.p999, st.max, st.mean,
		errors);
	fflush(f);
}

/* Reads one "Key:" field from an already-open /proc/self/status fd. Open the fd before any
 * restriction: Landlock denies the path afterwards, but reads on an open fd stay allowed. */
static inline int status_field(int fd, const char *key, char *out, size_t outlen)
{
	char buf[8192];
	if (lseek(fd, 0, SEEK_SET) < 0)
		return -1;
	ssize_t n = read(fd, buf, sizeof buf - 1);
	if (n < 0)
		return -1;
	buf[n] = 0;
	size_t kl = strlen(key);
	for (char *p = buf; p && *p;) {
		if (!strncmp(p, key, kl) && p[kl] == ':') {
			p += kl + 1;
			while (*p == ' ' || *p == '\t')
				p++;
			size_t i = 0;
			while (p[i] && p[i] != '\n' && i < outlen - 1) {
				out[i] = p[i];
				i++;
			}
			out[i] = 0;
			return 0;
		}
		p = strchr(p, '\n');
		if (p)
			p++;
	}
	return -1;
}

#endif
