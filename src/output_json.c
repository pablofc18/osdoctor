/*
 * output_json.c - machine-readable JSON report.
 *
 * Results are grouped contiguously by construction (each runner appends all of
 * its group's results in sequence), so we can emit nested group objects in a
 * single pass, opening a new group whenever the group name changes.
 */
#include "output.h"
#include "osdoctor.h"
#include "util_string.h"

#include <stdio.h>
#include <string.h>

/* Largest result string is detail (OSDOCTOR_DETAIL_CAP). Worst-case JSON
 * escaping expands each byte to 6 chars (\uXXXX), so size accordingly. */
static void emit_json_string(const char *s) {
    char buf[OSDOCTOR_DETAIL_CAP * 6 + 1];
    json_escape(s, buf, sizeof buf); /* always NUL-terminates */
    fputs(buf, stdout);
}

void output_json(const check_result_list_t *results) {
    check_summary_t s = summarize_results(results);

    printf("{\n");
    printf("  \"status\": \"%s\",\n", status_json(overall_status(s)));
    printf("  \"summary\": {\n");
    printf("    \"ok\": %d,\n", s.ok);
    printf("    \"warn\": %d,\n", s.warn);
    printf("    \"fail\": %d,\n", s.fail);
    printf("    \"skip\": %d\n", s.skip);
    printf("  },\n");
    printf("  \"groups\": [");

    const char *cur_group = NULL;
    for (size_t i = 0; i < results->len; i++) {
        const check_result_t *r = &results->items[i];
        bool new_group = (cur_group == NULL || strcmp(cur_group, r->group) != 0);
        if (new_group) {
            if (cur_group != NULL) {
                printf("\n      ]\n    },"); /* close previous group */
            }
            printf("\n    {\n      \"name\": \"");
            emit_json_string(r->group);
            printf("\",\n      \"checks\": [");
            cur_group = r->group;
        } else {
            printf(",");
        }

        printf("\n        {\n");
        printf("          \"id\": \"");
        emit_json_string(r->id);
        printf("\",\n");
        printf("          \"status\": \"%s\",\n", status_json(r->status));
        printf("          \"message\": \"");
        emit_json_string(r->message);
        printf("\"");
        if (r->detail[0] != '\0') {
            printf(",\n          \"detail\": \"");
            emit_json_string(r->detail);
            printf("\"");
        }
        printf("\n        }");
    }

    if (cur_group != NULL) {
        printf("\n      ]\n    }"); /* close final group */
    }
    printf("\n  ]\n");
    printf("}\n");
}
