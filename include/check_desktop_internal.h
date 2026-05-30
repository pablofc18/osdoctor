/*
 * check_desktop_internal.h - internals of the Desktop checks exposed for unit
 * testing. Not part of the public CLI surface.
 */
#ifndef OSDOCTOR_CHECK_DESKTOP_INTERNAL_H
#define OSDOCTOR_CHECK_DESKTOP_INTERNAL_H

#include <stddef.h>

/* Maximum `source =` include nesting depth before recursion is cut off. */
#define MAX_INCLUDE_DEPTH 32

/* ---- tiny de-duplicating string set ---- */

#define SET_CAP 64
#define SET_ITEM 256

typedef struct {
    char items[SET_CAP][SET_ITEM];
    size_t len;
} strset_t;

/* Result of scanning a Hyprland config (and everything it `source =`s) for
 * exec-bind commands. */
typedef struct {
    strset_t missing; /* checkable commands that did not resolve */
    int checked;      /* number of checkable commands seen */
} hypr_bind_scan_t;

/*
 * Parse exec-bind commands from `root_path` and every file it transitively
 * pulls in via `source =`. Resolves `~`, environment variables and relative
 * includes (against the including file's directory), expands globs, and guards
 * against include cycles and excessive nesting depth. `out` must be zeroed by
 * the caller.
 */
void hypr_scan_binds(const char *root_path, hypr_bind_scan_t *out);

#endif /* OSDOCTOR_CHECK_DESKTOP_INTERNAL_H */
