/*
 * check_packages.c - pacman database lock, log, and orphan packages.
 */
#include "checks.h"
#include "osdoctor.h"
#include "util_exec.h"
#include "util_fs.h"
#include "util_string.h"

#include <stdio.h>

static const char *GROUP = "Packages";

int run_package_checks(check_result_list_t *r) {
    /* 7. pacman database lock */
    if (file_exists("/var/lib/pacman/db.lck")) {
        if (results_add(
                r, "pacman-lock", GROUP, CHECK_FAIL, "pacman database is locked (db.lck present)",
                "Remove /var/lib/pacman/db.lck only if no pacman process is running.") != 0) {
            return -1;
        }
    } else if (results_add(r, "pacman-lock", GROUP, CHECK_OK, "pacman database is unlocked", "") !=
               0) {
        return -1;
    }

    /* 8. pacman log */
    if (file_readable("/var/log/pacman.log")) {
        if (results_add(r, "pacman-log", GROUP, CHECK_OK, "pacman log exists", "") != 0) {
            return -1;
        }
    } else if (results_add(r, "pacman-log", GROUP, CHECK_WARN, "pacman log missing or unreadable",
                           "") != 0) {
        return -1;
    }

    /* 9. Orphan packages (pacman -Qdtq exits 1 when there are none, so we rely
     * on whether any output was produced rather than the exit status). */
    if (!is_executable_in_path("pacman")) {
        return results_add(r, "orphan-packages", GROUP, CHECK_SKIP, "pacman not available", "");
    }

    char out[16384];
    int status = 0;
    run_capture("pacman -Qdtq 2>/dev/null", out, sizeof out, &status);

    size_t nlines = 0;
    char **lines = str_split_lines(out, &nlines);
    int count = 0;
    char detail[OSDOCTOR_DETAIL_CAP];
    detail[0] = '\0';
    size_t used = 0;
    for (size_t i = 0; i < nlines; i++) {
        char *line = str_trim(lines[i]);
        if (*line == '\0') {
            continue;
        }
        count++;
        if (used + 1 < sizeof detail) {
            int w =
                snprintf(detail + used, sizeof detail - used, "%s%s", (used > 0) ? ", " : "", line);
            if (w > 0) {
                used += (size_t)w;
                if (used >= sizeof detail) {
                    used = sizeof detail - 1;
                }
            }
        }
    }
    str_free_lines(lines, nlines);

    if (count == 0) {
        return results_add(r, "orphan-packages", GROUP, CHECK_OK, "No orphan packages", "");
    }
    char msg[OSDOCTOR_MSG_CAP];
    snprintf(msg, sizeof msg, "%d orphan package%s detected", count, (count == 1) ? "" : "s");
    return results_add(r, "orphan-packages", GROUP, CHECK_WARN, msg, detail);
}
