/*
 * check_packages.c - pacman database lock, log, and orphan packages.
 *
 * The orphan-package lookup sits behind a small backend seam. pkg_query_pacman
 * shells out to `pacman -Qdtq`; when built with USE_LIBALPM, pkg_query_alpm (in
 * check_packages_alpm.c) reads the local pacman database directly and is tried
 * first, falling back to pacman only on a hard error. pkg_emit turns either
 * backend's result into the check output, so both share wording.
 */
#include "check_packages_internal.h"
#include "checks.h"
#include "osdoctor.h"
#include "util_exec.h"
#include "util_fs.h"
#include "util_string.h"

#include <stdio.h>

static const char *GROUP = "Packages";

/*
 * Append a package name to a bounded, comma-separated sample in `detail`. The
 * comma-join and truncation behaviour matches what the inline orphan loop
 * produced before the backends were split out; the libalpm backend reuses it.
 */
void pkg_detail_append(char *detail, size_t detail_size, size_t *used, const char *name) {
    if (detail == NULL || name == NULL || used == NULL || *used + 1 >= detail_size) {
        return;
    }
    int w = snprintf(detail + *used, detail_size - *used, "%s%s", (*used > 0) ? ", " : "", name);
    if (w > 0) {
        *used += (size_t)w;
        if (*used >= detail_size) {
            *used = detail_size - 1;
        }
    }
}

/*
 * Translate a backend query result into the orphan-packages check result. The
 * id, group, status and message text match the documented output (see
 * examples/sample-output.*), so the result is identical regardless of which
 * backend produced the data.
 */
int pkg_emit(check_result_list_t *r, pkg_query_status_t status, int count, const char *detail) {
    if (status == PKG_QUERY_UNAVAILABLE) {
        return results_add(r, "orphan-packages", GROUP, CHECK_SKIP, "pacman not available", "");
    }
    if (count <= 0) {
        return results_add(r, "orphan-packages", GROUP, CHECK_OK, "No orphan packages", "");
    }
    char msg[OSDOCTOR_MSG_CAP];
    snprintf(msg, sizeof msg, "%d orphan package%s detected", count, (count == 1) ? "" : "s");
    return results_add(r, "orphan-packages", GROUP, CHECK_WARN, msg, detail);
}

/*
 * pacman backend: parse `pacman -Qdtq`. The command exits 1 when there are no
 * orphans, so the result is judged by whether any output was produced rather
 * than by the exit status. Returns PKG_QUERY_UNAVAILABLE when pacman is not in
 * PATH (which pkg_emit turns into a SKIP).
 */
static pkg_query_status_t pkg_query_pacman(int *count, char *detail, size_t detail_size) {
    *count = 0;
    if (detail_size > 0) {
        detail[0] = '\0';
    }
    if (!is_executable_in_path("pacman")) {
        return PKG_QUERY_UNAVAILABLE;
    }

    char out[16384];
    int status = 0;
    run_capture("pacman -Qdtq 2>/dev/null", out, sizeof out, &status);

    size_t nlines = 0;
    char **lines = str_split_lines(out, &nlines);
    int n = 0;
    size_t used = 0;
    for (size_t i = 0; i < nlines; i++) {
        char *line = str_trim(lines[i]);
        if (*line == '\0') {
            continue;
        }
        n++;
        pkg_detail_append(detail, detail_size, &used, line);
    }
    str_free_lines(lines, nlines);

    *count = n;
    return PKG_QUERY_OK;
}

/*
 * Enumerate orphans, preferring libalpm when compiled in. An OK result is
 * authoritative; only a hard error (UNAVAILABLE) falls back to the pacman
 * backend.
 */
static pkg_query_status_t pkg_query_orphans(int *count, char *detail, size_t detail_size) {
#ifdef OSDOCTOR_HAVE_LIBALPM
    pkg_query_status_t st = pkg_query_alpm(count, detail, detail_size);
    if (st != PKG_QUERY_UNAVAILABLE) {
        return st;
    }
#endif
    return pkg_query_pacman(count, detail, detail_size);
}

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

    /* 9. Orphan packages */
    char detail[OSDOCTOR_DETAIL_CAP];
    int count = 0;
    pkg_query_status_t st = pkg_query_orphans(&count, detail, sizeof detail);
    return pkg_emit(r, st, count, detail);
}
