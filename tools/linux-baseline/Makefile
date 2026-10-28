# Linux baseline benchmark harness. gcc and glibc only, no other library.
CC      = gcc
CFLAGS  = -O2 -g -std=gnu11 -Wall -Wextra -Werror -D_GNU_SOURCE
BIN     = bin

all: $(BIN)/micro $(BIN)/broker $(BIN)/deadline $(BIN)/cgbench $(BIN)/floor

$(BIN):
	mkdir -p $(BIN)

$(BIN)/micro: micro.c sandbox.c sandbox.h common.h uapi_compat.h | $(BIN)
	$(CC) $(CFLAGS) -o $@ micro.c sandbox.c

$(BIN)/broker: broker.c common.h uapi_compat.h | $(BIN)
	$(CC) $(CFLAGS) -o $@ broker.c

$(BIN)/deadline: deadline.c cg.c cg.h common.h uapi_compat.h | $(BIN)
	$(CC) $(CFLAGS) -o $@ deadline.c cg.c

$(BIN)/cgbench: cgbench.c cg.c cg.h common.h uapi_compat.h | $(BIN)
	$(CC) $(CFLAGS) -o $@ cgbench.c cg.c

$(BIN)/floor: floor.c common.h uapi_compat.h | $(BIN)
	$(CC) $(CFLAGS) -o $@ floor.c

clean:
	rm -rf $(BIN)

.PHONY: all clean
