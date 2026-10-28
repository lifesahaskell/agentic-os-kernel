# BPF-LSM objects for the Phase 8 measurements. Reference host only (R): the vmlinux.h rule needs
# bpftool and a BTF kernel (/sys/kernel/btf/vmlinux), neither present on WSL2. Objects and vmlinux.h are
# generated and gitignored.
#
#   make -C tools/bpflsm                  the six objects, lsm_cgroup socket_sendmsg program off
#   make -B -C tools/bpflsm CG_SENDMSG=1  also builds the lsm_cgroup socket_sendmsg program
#
# CG_SENDMSG is empty by default (OQ7 default no). OQ7 is answered in 08-04, wave 2, after this wave-1
# plan, so 08-05 Task 2 rebuilds with make -B CG_SENDMSG=1 if the OQ7 Answer is yes. -B is needed
# because make does not see a changed variable as a stale bpflsm_cg.bpf.o.
#
# ANC_LEVEL is the depth of /sys/fs/cgroup/aok-phase8 under the cgroup2 root, used only by
# bpflsm_anc.bpf.o. bpflsm.sh keys the aok-phase8 id for the anc variant on that assumption.
CLANG ?= clang
BPFTOOL ?= bpftool
CG_SENDMSG ?=
ANC_LEVEL ?= 1
BPF_CFLAGS := -target bpf -g -O2 -Wall -D__TARGET_ARCH_x86 -I.

all: bpflsm.bpf.o bpflsm_nocount.bpf.o bpflsm_anc.bpf.o bpflsm_cg.bpf.o bpfadv.bpf.o probe.bpf.o

vmlinux.h:
	$(BPFTOOL) btf dump file /sys/kernel/btf/vmlinux format c > $@ || { rm -f $@; exit 1; }

bpflsm.bpf.o: bpflsm.bpf.c vmlinux.h
	$(CLANG) $(BPF_CFLAGS) -c $< -o $@

bpflsm_nocount.bpf.o: bpflsm.bpf.c vmlinux.h
	$(CLANG) $(BPF_CFLAGS) -DAOK_NO_COUNTER -c $< -o $@

bpflsm_anc.bpf.o: bpflsm.bpf.c vmlinux.h
	$(CLANG) $(BPF_CFLAGS) -DAOK_KEY_ANCESTOR -DAOK_ANC_LEVEL=$(ANC_LEVEL) -c $< -o $@

bpflsm_cg.bpf.o: bpflsm_cg.bpf.c vmlinux.h
	$(CLANG) $(BPF_CFLAGS) $(if $(filter 1,$(CG_SENDMSG)),-DAOK_CG_SENDMSG) -c $< -o $@

bpfadv.bpf.o: bpfadv.bpf.c vmlinux.h
	$(CLANG) $(BPF_CFLAGS) -c $< -o $@

probe.bpf.o: probe.bpf.c vmlinux.h
	$(CLANG) $(BPF_CFLAGS) -c $< -o $@

clean:
	rm -f *.bpf.o vmlinux.h

.PHONY: all clean
