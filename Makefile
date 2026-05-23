# osdoctor - Linux desktop health checks for Arch/Hyprland systems
#
# Common targets:
#   make            build the osdoctor binary
#   make test       build and run unit tests
#   make clean      remove build artifacts
#   make install    install to $(PREFIX) (default /usr/local)
#   make uninstall  remove installed files
#   make format     run clang-format over the sources (if available)
#
# Requires a C23-capable compiler (GCC >= 14 or Clang >= 18). Override the
# standard with e.g. `make CSTD=c2x` for slightly older toolchains.

CC      ?= cc
PREFIX  ?= /usr/local
BINDIR   = $(PREFIX)/bin
MANDIR   = $(PREFIX)/share/man/man1

CSTD    ?= c23
CFLAGS  ?= -O2 -g
# Mandatory flags (use override so a user-supplied CFLAGS can't drop them).
override CFLAGS   += -std=$(CSTD) -Wall -Wextra -Wpedantic
override CPPFLAGS += -Iinclude -D_POSIX_C_SOURCE=200809L -D_DEFAULT_SOURCE

BIN      = osdoctor
SRC_DIR  = src
OBJ_DIR  = build
TEST_DIR = tests
MAN_PAGE = man/osdoctor.1

# Library sources: everything except main.c. Shared between the binary and the
# test executables (which provide their own main()).
LIB_SRCS = \
	$(SRC_DIR)/cli.c \
	$(SRC_DIR)/checks.c \
	$(SRC_DIR)/output_text.c \
	$(SRC_DIR)/output_json.c \
	$(SRC_DIR)/util_exec.c \
	$(SRC_DIR)/util_fs.c \
	$(SRC_DIR)/util_string.c \
	$(SRC_DIR)/check_system.c \
	$(SRC_DIR)/check_services.c \
	$(SRC_DIR)/check_packages.c \
	$(SRC_DIR)/check_desktop.c

ALL_SRCS = $(SRC_DIR)/main.c $(LIB_SRCS)
OBJS     = $(patsubst $(SRC_DIR)/%.c,$(OBJ_DIR)/%.o,$(ALL_SRCS))
LIB_OBJS = $(patsubst $(SRC_DIR)/%.c,$(OBJ_DIR)/%.o,$(LIB_SRCS))

TESTS     = test_string test_fs
TEST_BINS = $(addprefix $(OBJ_DIR)/,$(TESTS))

HEADERS = $(wildcard include/*.h)

.PHONY: all clean test install uninstall format
.DEFAULT_GOAL := all

all: $(BIN)

$(BIN): $(OBJS)
	$(CC) $(CFLAGS) $(LDFLAGS) -o $@ $^

$(OBJ_DIR)/%.o: $(SRC_DIR)/%.c $(HEADERS) | $(OBJ_DIR)
	$(CC) $(CFLAGS) $(CPPFLAGS) -c -o $@ $<

$(OBJ_DIR):
	mkdir -p $(OBJ_DIR)

# ---- tests ----

test: $(TEST_BINS)
	@fail=0; \
	for t in $(TEST_BINS); do \
		echo "== running $$t =="; \
		./$$t || fail=1; \
	done; \
	if [ $$fail -ne 0 ]; then echo "TESTS FAILED"; exit 1; fi; \
	echo "ALL TESTS PASSED"

$(OBJ_DIR)/test_string: $(TEST_DIR)/test_string.c $(LIB_OBJS) $(HEADERS) | $(OBJ_DIR)
	$(CC) $(CFLAGS) $(CPPFLAGS) -o $@ $(TEST_DIR)/test_string.c $(LIB_OBJS) $(LDFLAGS)

$(OBJ_DIR)/test_fs: $(TEST_DIR)/test_fs.c $(LIB_OBJS) $(HEADERS) | $(OBJ_DIR)
	$(CC) $(CFLAGS) $(CPPFLAGS) -o $@ $(TEST_DIR)/test_fs.c $(LIB_OBJS) $(LDFLAGS)

# ---- install / uninstall ----

install: $(BIN)
	install -Dm755 $(BIN) $(DESTDIR)$(BINDIR)/$(BIN)
	install -Dm644 $(MAN_PAGE) $(DESTDIR)$(MANDIR)/osdoctor.1

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/$(BIN)
	rm -f $(DESTDIR)$(MANDIR)/osdoctor.1

# ---- misc ----

format:
	@if command -v clang-format >/dev/null 2>&1; then \
		clang-format -i $(SRC_DIR)/*.c include/*.h $(TEST_DIR)/*.c && echo "formatted"; \
	else \
		echo "clang-format not found; skipping"; \
	fi

clean:
	rm -rf $(OBJ_DIR) $(BIN)
