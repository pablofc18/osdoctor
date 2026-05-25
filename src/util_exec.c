#include "util_exec.h"

#include <stdio.h>
#include <string.h>
#include <sys/wait.h>

int run_capture(const char *cmd, char *out, size_t out_size, int *exit_status) {
    if (exit_status != NULL) {
        *exit_status = -1;
    }
    if (out != NULL && out_size > 0) {
        out[0] = '\0';
    }

    FILE *fp = popen(cmd, "r");
    if (fp == NULL) {
        return -1;
    }

    size_t total = 0;
    char chunk[4096];
    size_t n;
    while ((n = fread(chunk, 1, sizeof chunk, fp)) > 0) {
        if (out != NULL && out_size > 0 && total < out_size - 1) {
            size_t space = (out_size - 1) - total;
            size_t copy = (n < space) ? n : space;
            memcpy(out + total, chunk, copy);
            total += copy;
        }
        /* Keep draining beyond the buffer so the child never blocks on write. */
    }
    if (out != NULL && out_size > 0) {
        out[total] = '\0';
    }

    int status = pclose(fp);
    if (status != -1 && exit_status != NULL) {
        *exit_status = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
    }
    return 0;
}
