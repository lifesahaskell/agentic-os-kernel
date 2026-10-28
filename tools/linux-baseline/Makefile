# Linux baseline benchmark harness. gcc and glibc only, no other library.
CC      = gcc
CFLAGS  = -O2 -g -std=gnu11 -Wall -Wextra -Werror -D_GNU_SOURCE
BIN     = bin

all: $(BIN)/micro

$(BIN):
	mkdir -p $(BIN)

$(BIN)/micro: micro.c sandbox.c sandbox.h common.h uapi_compat.h | $(BIN)
	$(CC) $(CFLAGS) -o $@ micro.c sandbox.c

clean:
	rm -rf $(BIN)

.PHONY: all clean
