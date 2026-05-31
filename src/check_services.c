/*
 * check_services.c - failed systemd units (system and user scope).
 *
 * The failed-unit lookup sits behind a small backend seam. svc_query_systemctl
 * shells out to `systemctl --failed` (fixed argument lists, stderr redirected
 * to /dev/null). When built with USE_LIBSYSTEMD, svc_query_sdbus (in
 * check_services_sdbus.c) queries the systemd manager directly over D-Bus and
 * is tried first, falling back to systemctl only on a hard bus error. svc_emit
 * turns either backend's result into the check output, so both share wording.
 */
#include "check_services_internal.h"
#include "checks.h"
#include "osdoctor.h"
#include "util_exec.h"
#include "util_fs.h"
#include "util_string.h"

#include <stdio.h>

static const char *GROUP = "Services";

/*
 * Append a unit name to a bounded, comma-separated sample in `detail`. The
 * comma-join and truncation behaviour matches what count_units() produced
 * before the backends were split out; the sd-bus backend reuses it too.
 */
void svc_detail_append(char *detail, size_t detail_size, size_t *used, const char *unit) {
    if (detail == NULL || unit == NULL || used == NULL || *used + 1 >= detail_size) {
        return;
    }
    int w = snprintf(detail + *used, detail_size - *used, "%s%s", (*used > 0) ? ", " : "", unit);
    if (w > 0) {
        *used += (size_t)w;
        if (*used >= detail_size) {
            *used = detail_size - 1;
        }
    }
}

/*
 * Translate a backend query result for `scope` into a check result. The id,
 * group, status and message text are chosen to match the documented output
 * (see examples/sample-output.*), so the result is identical regardless of
 * which backend produced the data.
 */
int svc_emit(check_result_list_t *r, svc_scope_t scope, svc_query_status_t status, int count,
             const char *detail) {
    const char *id = (scope == SVC_SCOPE_SYSTEM) ? "failed-system-services" : "failed-user-services";
    const char *noun = (scope == SVC_SCOPE_SYSTEM) ? "system" : "user";
    char msg[OSDOCTOR_MSG_CAP];

    switch (status) {
    case SVC_QUERY_OK:
        if (count <= 0) {
            snprintf(msg, sizeof msg, "No failed %s services", noun);
            return results_add(r, id, GROUP, CHECK_OK, msg, "");
        }
        snprintf(msg, sizeof msg, "%d failed %s service%s", count, noun, (count == 1) ? "" : "s");
        return results_add(r, id, GROUP, CHECK_WARN, msg, detail);
    case SVC_QUERY_NO_SESSION:
        if (scope == SVC_SCOPE_USER) {
            return results_add(r, id, GROUP, CHECK_SKIP, "No systemd user session available", "");
        }
        return results_add(r, id, GROUP, CHECK_SKIP, "No system bus available", "");
    case SVC_QUERY_UNAVAILABLE:
    default:
        return results_add(r, id, GROUP, CHECK_SKIP, "systemctl not available", "");
    }
}

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
        if (sscanf(line, "%255s", unit) == 1) {
            svc_detail_append(detail, detail_size, &used, unit);
        }
    }
    str_free_lines(lines, nlines);
    return count;
}

/*
 * systemctl backend: parse `systemctl [--user] --failed`. System scope ignores
 * the exit status (an empty list is the normal "nothing failed" case); a
 * non-zero user-scope exit means there is no usable user manager/bus.
 */
static svc_query_status_t svc_query_systemctl(svc_scope_t scope, int *count, char *detail,
                                              size_t detail_size) {
    *count = 0;
    if (detail_size > 0) {
        detail[0] = '\0';
    }
    if (!is_executable_in_path("systemctl")) {
        return SVC_QUERY_UNAVAILABLE;
    }

    const char *cmd = (scope == SVC_SCOPE_SYSTEM)
                          ? "systemctl --failed --no-legend --plain 2>/dev/null"
                          : "systemctl --user --failed --no-legend --plain 2>/dev/null";
    char out[8192];
    int status = 0;
    run_capture(cmd, out, sizeof out, &status);

    if (scope == SVC_SCOPE_USER && status != 0) {
        return SVC_QUERY_NO_SESSION;
    }
    *count = count_units(out, detail, detail_size);
    return SVC_QUERY_OK;
}

/*
 * Query one scope, preferring sd-bus when compiled in. OK and NO_SESSION are
 * authoritative and returned as-is; only a hard sd-bus error (UNAVAILABLE)
 * falls back to the systemctl backend.
 */
static svc_query_status_t svc_query(svc_scope_t scope, int *count, char *detail,
                                    size_t detail_size) {
#ifdef OSDOCTOR_HAVE_LIBSYSTEMD
    svc_query_status_t st = svc_query_sdbus(scope, count, detail, detail_size);
    if (st != SVC_QUERY_UNAVAILABLE) {
        return st;
    }
#endif
    return svc_query_systemctl(scope, count, detail, detail_size);
}

int run_service_checks(check_result_list_t *r) {
    char detail[OSDOCTOR_DETAIL_CAP];
    int count = 0;

    svc_query_status_t st = svc_query(SVC_SCOPE_SYSTEM, &count, detail, sizeof detail);
    if (svc_emit(r, SVC_SCOPE_SYSTEM, st, count, detail) != 0) {
        return -1;
    }

    st = svc_query(SVC_SCOPE_USER, &count, detail, sizeof detail);
    if (svc_emit(r, SVC_SCOPE_USER, st, count, detail) != 0) {
        return -1;
    }
    return 0;
}
