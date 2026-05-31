/*
 * check_packages_internal.h - internals of the Packages checks exposed for
 * unit testing and shared between the orchestrator and the optional libalpm
 * backend. Not part of the public CLI surface.
 */
#ifndef OSDOCTOR_CHECK_PACKAGES_INTERNAL_H
#define OSDOCTOR_CHECK_PACKAGES_INTERNAL_H

#include "osdoctor.h"

#include <stddef.h>

/* Outcome of asking a backend to enumerate orphan packages. */
typedef enum {
    PKG_QUERY_OK = 0,      /* authoritative answer: *count and detail filled   */
    PKG_QUERY_UNAVAILABLE, /* backend can't run / hard error  -> try fallback  */
} pkg_query_status_t;

/*
 * Append a package name to a bounded, comma-separated sample in `detail`.
 * `*used` tracks the current length and is updated; nothing is written once the
 * buffer is full, so `detail` stays NUL-terminated. No-op on NULL inputs.
 * The caller must initialise `detail[0] = '\0'` and `*used = 0`.
 */
void pkg_detail_append(char *detail, size_t detail_size, size_t *used, const char *name);

/*
 * Map a backend query result onto the `orphan-packages` check result and append
 * it to `r` (id, group, status and message chosen to match the documented
 * output). Returns 0 on success or -1 on allocation failure.
 */
int pkg_emit(check_result_list_t *r, pkg_query_status_t status, int count, const char *detail);

#ifdef OSDOCTOR_HAVE_LIBALPM
/*
 * Enumerate orphan packages by reading the local pacman DB via libalpm. Fills
 * *count (all orphans) and `detail` (a bounded comma-separated sample of names).
 * Returns PKG_QUERY_OK on success, or PKG_QUERY_UNAVAILABLE on any hard error
 * (init/db failure) so the caller falls back to the pacman shell-out.
 */
pkg_query_status_t pkg_query_alpm(int *count, char *detail, size_t detail_size);
#endif

#endif /* OSDOCTOR_CHECK_PACKAGES_INTERNAL_H */
