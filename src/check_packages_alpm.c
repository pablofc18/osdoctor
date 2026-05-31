/*
 * check_packages_alpm.c - libalpm backend for the orphan-package check.
 *
 * Compiled only when built with USE_LIBALPM=1. Reads the local pacman database
 * directly instead of shelling out to `pacman -Qdtq`. A package is an orphan
 * when it was installed as a dependency and nothing (neither a hard dependency
 * nor an optional one) still requires it - the same set `pacman -Qdt` reports.
 */
#include "check_packages_internal.h"

#include <alpm.h>
#include <alpm_list.h>

pkg_query_status_t pkg_query_alpm(int *count, char *detail, size_t detail_size) {
    *count = 0;
    if (detail_size > 0) {
        detail[0] = '\0';
    }

    alpm_errno_t err = 0;
    alpm_handle_t *handle = alpm_initialize("/", "/var/lib/pacman/", &err);
    if (handle == NULL) {
        return PKG_QUERY_UNAVAILABLE;
    }

    pkg_query_status_t result = PKG_QUERY_UNAVAILABLE;
    alpm_db_t *db = alpm_get_localdb(handle);
    if (db == NULL) {
        goto finish;
    }
    alpm_list_t *pkgs = alpm_db_get_pkgcache(db); /* owned by handle, not freed */
    if (pkgs == NULL) {
        /* An empty local DB is unusual but valid: report zero orphans. */
        result = PKG_QUERY_OK;
        goto finish;
    }

    int n = 0;
    size_t used = 0;
    for (alpm_list_t *i = pkgs; i != NULL; i = alpm_list_next(i)) {
        alpm_pkg_t *pkg = i->data;
        if (alpm_pkg_get_reason(pkg) != ALPM_PKG_REASON_DEPEND) {
            continue;
        }
        alpm_list_t *requiredby = alpm_pkg_compute_requiredby(pkg);
        alpm_list_t *optionalfor = alpm_pkg_compute_optionalfor(pkg);
        int is_orphan = (requiredby == NULL && optionalfor == NULL);
        FREELIST(requiredby);
        FREELIST(optionalfor);
        if (!is_orphan) {
            continue;
        }
        n++;
        pkg_detail_append(detail, detail_size, &used, alpm_pkg_get_name(pkg));
    }
    *count = n;
    result = PKG_QUERY_OK;

finish:
    alpm_release(handle);
    return result;
}
