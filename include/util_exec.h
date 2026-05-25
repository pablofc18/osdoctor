/*
 * util_exec.h - safe-ish command execution.
 *
 * SECURITY: run_capture() passes `cmd` to popen() and therefore to /bin/sh.
 * Only ever call it with fixed, hard-coded command strings. Never build the
 * command from user-controlled or filesystem-derived input.
 */
#ifndef OSDOCTOR_UTIL_EXEC_H
#define OSDOCTOR_UTIL_EXEC_H

#include <stddef.h>

/*
 * Run `cmd` via popen("r"), capturing up to out_size-1 bytes of stdout into
 * `out` (always NUL-terminated). Any output beyond the buffer is drained and
 * discarded so the child can finish. If `exit_status` is non-NULL it is set to
 * the child's exit status (WEXITSTATUS) when it exited normally, or -1.
 *
 * Returns 0 if the command was started, -1 if popen() failed.
 */
int run_capture(const char *cmd, char *out, size_t out_size, int *exit_status);

#endif /* OSDOCTOR_UTIL_EXEC_H */
