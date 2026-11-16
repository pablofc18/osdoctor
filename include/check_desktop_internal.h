/*
 * check_desktop_internal.h - internals of the Desktop checks exposed for unit
 * testing. Not part of the public CLI surface.
 */
#ifndef OSDOCTOR_CHECK_DESKTOP_INTERNAL_H
#define OSDOCTOR_CHECK_DESKTOP_INTERNAL_H

#include <stdbool.h>
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
    bool root_read;   /* the root config itself was read; false means the scan
                       * saw nothing and must not be reported as resolved */
} hypr_bind_scan_t;

/*
 * Parse exec-bind commands from `root_path` and every file it transitively
 * pulls in via `source =`. Resolves `~`, environment variables and relative
 * includes (against the including file's directory), expands globs, and guards
 * against include cycles and excessive nesting depth. `out` must be zeroed by
 * the caller.
 */
void hypr_scan_binds(const char *root_path, hypr_bind_scan_t *out);

/*
 * Scan Waybar config text for references to local scripts: every token of
 * every double-quoted string that looks like a local script path is counted in
 * *checked and, when it does not exist (relative paths resolve against
 * `config_dir`), added to `missing`. JSONC `//` and block comments are
 * skipped. `missing` and `*checked` must be zeroed by the caller.
 */
void waybar_scan_scripts(const char *content, const char *config_dir, strset_t *missing,
                         int *checked);

#endif /* OSDOCTOR_CHECK_DESKTOP_INTERNAL_H */
