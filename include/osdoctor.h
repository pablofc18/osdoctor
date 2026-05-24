/*
 * osdoctor.h - core data structures and result/summary helpers.
 *
 * These types are the spine of the program: every check produces a
 * check_result_t, runners append them to a check_result_list_t, and the
 * output and exit-code logic consume the list.
 */
#ifndef OSDOCTOR_H
#define OSDOCTOR_H

#include <stdbool.h>
#include <stddef.h>

#define OSDOCTOR_VERSION "0.1.0"

/* Fixed buffer sizes for result strings. Messages and details are copied
 * (truncated if necessary) into these buffers, so results never alias caller
 * memory beyond the id/group pointers documented below. */
#define OSDOCTOR_MSG_CAP 512
#define OSDOCTOR_DETAIL_CAP 1024

typedef enum { CHECK_OK = 0, CHECK_WARN, CHECK_FAIL, CHECK_SKIP } check_status_t;

/*
 * A single check outcome.
 *
 * Ownership: `id` and `group` are borrowed pointers and MUST point at
 * string literals (or otherwise static-lifetime storage). They are stored
 * by pointer, not copied. `message` and `detail` are copied into the
 * fixed-size buffers by results_add().
 */
typedef struct {
    const char *id;    /* stable machine id, e.g. "root-disk-space"  */
    const char *group; /* group label, e.g. "System"                 */
    check_status_t status;
    char message[OSDOCTOR_MSG_CAP];
    char detail[OSDOCTOR_DETAIL_CAP];
} check_result_t;

/* Growable array of results. */
typedef struct {
    check_result_t *items;
    size_t len;
    size_t cap;
} check_result_list_t;

typedef struct {
    int ok;
    int warn;
    int fail;
    int skip;
} check_summary_t;

/* ---- result list lifecycle ---- */

void results_init(check_result_list_t *list);
void results_free(check_result_list_t *list);

/*
 * Append a result. Returns 0 on success, -1 on allocation failure.
 * `id` and `group` must be static-lifetime strings (stored by pointer).
 * `message`/`detail` may be NULL (treated as empty) and are copied.
 */
int results_add(check_result_list_t *list, const char *id, const char *group, check_status_t status,
                const char *message, const char *detail);

/* ---- summary & exit codes ---- */

check_summary_t summarize_results(const check_result_list_t *results);

/*
 * Map a summary to a process exit code:
 *   any fail            -> 2
 *   any warn (no fail)  -> 1   (or 2 when strict)
 *   otherwise           -> 0
 */
int exit_code_from_summary(check_summary_t summary, bool strict);

/* Worst status present (fail > warn > ok); skips do not count. */
check_status_t overall_status(check_summary_t summary);

/* ---- status string helpers ---- */

const char *status_label(check_status_t status);  /* "OK" / "WARN" / ...  */
const char *status_json(check_status_t status);   /* "ok" / "warn" / ...  */
const char *status_symbol(check_status_t status); /* glyph for text output */

#endif /* OSDOCTOR_H */
