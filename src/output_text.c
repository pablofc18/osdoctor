/*
 * output_text.c - human-readable, grouped terminal report.
 */
#include "output.h"
#include "osdoctor.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static bool use_color(void) {
    if (getenv("NO_COLOR") != NULL) {
        return false;
    }
    return isatty(STDOUT_FILENO) == 1;
}

static const char *color_for(check_status_t status) {
    switch (status) {
    case CHECK_OK:
        return "\x1b[32m"; /* green  */
    case CHECK_WARN:
        return "\x1b[33m"; /* yellow */
    case CHECK_FAIL:
        return "\x1b[31m"; /* red    */
    case CHECK_SKIP:
        return "\x1b[90m"; /* grey   */
    }
    return "";
}

void output_text(const check_result_list_t *results) {
    bool color = use_color();
    const char *reset = color ? "\x1b[0m" : "";
    const char *dim = color ? "\x1b[90m" : "";

    const char *prev_group = NULL;
    for (size_t i = 0; i < results->len; i++) {
        const check_result_t *r = &results->items[i];
        if (prev_group == NULL || strcmp(prev_group, r->group) != 0) {
            printf("%s%s\n", (i == 0) ? "" : "\n", r->group);
            prev_group = r->group;
        }
        const char *col = color ? color_for(r->status) : "";
        printf("  %s%s%s %s\n", col, status_symbol(r->status), reset, r->message);
        if (r->detail[0] != '\0') {
            printf("      %s%s%s\n", dim, r->detail, reset);
        }
    }

    check_summary_t s = summarize_results(results);
    check_status_t overall = overall_status(s);
    const char *ocol = color ? color_for(overall) : "";

    char counts[256];
    snprintf(counts, sizeof counts, "%d ok, %d warning%s, %d failure%s", s.ok, s.warn,
             (s.warn == 1) ? "" : "s", s.fail, (s.fail == 1) ? "" : "s");
    if (s.skip > 0) {
        char extra[64];
        snprintf(extra, sizeof extra, ", %d skipped", s.skip);
        strncat(counts, extra, sizeof counts - strlen(counts) - 1);
    }

    printf("\nSummary\n");
    printf("  Status: %s%s%s\n", ocol, status_label(overall), reset);
    printf("  Checks: %s\n", counts);
}
