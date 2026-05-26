/*
 * check_services.c - failed systemd units (system and user scope).
 *
 * We shell out to systemctl with fixed argument lists. stderr is redirected
 * to /dev/null so a missing user bus doesn't leak noise to the terminal; we
 * detect that case via the child exit status instead.
 */
#include "checks.h"
#include "osdoctor.h"
#include "util_exec.h"
#include "util_fs.h"
#include "util_string.h"

#include <stdio.h>

static const char *GROUP = "Services";

/*
 * Count non-empty lines in `out` and collect the first whitespace-delimited
 * token (the unit name) of each into `detail` as a comma-separated list.
 */
static int count_units(const char *out, char *detail, size_t detail_size) {
    if (detail_size > 0) {
        detail[0] = '\0';
    }
    size_t nlines = 0;
    char **lines = str_split_lines(out, &nlines);
    int count = 0;
    size_t used = 0;
    for (size_t i = 0; i < nlines; i++) {
        char *line = str_trim(lines[i]);
        if (*line == '\0') {
            continue;
        }
        count++;
        char unit[256];
        if (sscanf(line, "%255s", unit) == 1 && used + 1 < detail_size) {
            int w =
                snprintf(detail + used, detail_size - used, "%s%s", (used > 0) ? ", " : "", unit);
            if (w > 0) {
                used += (size_t)w;
                if (used >= detail_size) {
                    used = detail_size - 1;
                }
            }
        }
    }
    str_free_lines(lines, nlines);
    return count;
}

int run_service_checks(check_result_list_t *r) {
    char out[8192];
    char detail[OSDOCTOR_DETAIL_CAP];
    char msg[OSDOCTOR_MSG_CAP];

    if (!is_executable_in_path("systemctl")) {
        if (results_add(r, "failed-system-services", GROUP, CHECK_SKIP, "systemctl not available",
                        "") != 0) {
            return -1;
        }
        if (results_add(r, "failed-user-services", GROUP, CHECK_SKIP, "systemctl not available",
                        "") != 0) {
            return -1;
        }
        return 0;
    }

    /* 5. Failed system services */
    int status = 0;
    run_capture("systemctl --failed --no-legend --plain 2>/dev/null", out, sizeof out, &status);
    int n = count_units(out, detail, sizeof detail);
    if (n == 0) {
        if (results_add(r, "failed-system-services", GROUP, CHECK_OK, "No failed system services",
                        "") != 0) {
            return -1;
        }
    } else {
        snprintf(msg, sizeof msg, "%d failed system service%s", n, (n == 1) ? "" : "s");
        if (results_add(r, "failed-system-services", GROUP, CHECK_WARN, msg, detail) != 0) {
            return -1;
        }
    }

    /* 6. Failed user services */
    status = 0;
    run_capture("systemctl --user --failed --no-legend --plain 2>/dev/null", out, sizeof out,
                &status);
    if (status != 0) {
        /* No usable user manager / bus for this session. */
        if (results_add(r, "failed-user-services", GROUP, CHECK_SKIP,
                        "No systemd user session available", "") != 0) {
            return -1;
        }
    } else {
        n = count_units(out, detail, sizeof detail);
        if (n == 0) {
            if (results_add(r, "failed-user-services", GROUP, CHECK_OK, "No failed user services",
                            "") != 0) {
                return -1;
            }
        } else {
            snprintf(msg, sizeof msg, "%d failed user service%s", n, (n == 1) ? "" : "s");
            if (results_add(r, "failed-user-services", GROUP, CHECK_WARN, msg, detail) != 0) {
                return -1;
            }
        }
    }

    return 0;
}
