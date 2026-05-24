#include "checks.h"
#include "osdoctor.h"

#include <stdio.h>
#include <stdlib.h>

void results_init(check_result_list_t *list) {
    list->items = NULL;
    list->len = 0;
    list->cap = 0;
}

void results_free(check_result_list_t *list) {
    free(list->items);
    list->items = NULL;
    list->len = 0;
    list->cap = 0;
}

int results_add(check_result_list_t *list, const char *id, const char *group, check_status_t status,
                const char *message, const char *detail) {
    if (list->len == list->cap) {
        size_t ncap = (list->cap != 0) ? list->cap * 2 : 8;
        check_result_t *grown = realloc(list->items, ncap * sizeof *grown);
        if (grown == NULL) {
            return -1;
        }
        list->items = grown;
        list->cap = ncap;
    }

    check_result_t *r = &list->items[list->len++];
    r->id = id;
    r->group = group;
    r->status = status;
    snprintf(r->message, sizeof r->message, "%s", (message != NULL) ? message : "");
    snprintf(r->detail, sizeof r->detail, "%s", (detail != NULL) ? detail : "");
    return 0;
}

check_summary_t summarize_results(const check_result_list_t *results) {
    check_summary_t s = {0};
    for (size_t i = 0; i < results->len; i++) {
        switch (results->items[i].status) {
        case CHECK_OK:
            s.ok++;
            break;
        case CHECK_WARN:
            s.warn++;
            break;
        case CHECK_FAIL:
            s.fail++;
            break;
        case CHECK_SKIP:
            s.skip++;
            break;
        }
    }
    return s;
}

int exit_code_from_summary(check_summary_t summary, bool strict) {
    if (summary.fail > 0) {
        return 2;
    }
    if (summary.warn > 0) {
        return strict ? 2 : 1;
    }
    return 0;
}

check_status_t overall_status(check_summary_t summary) {
    if (summary.fail > 0) {
        return CHECK_FAIL;
    }
    if (summary.warn > 0) {
        return CHECK_WARN;
    }
    return CHECK_OK;
}

const char *status_label(check_status_t status) {
    switch (status) {
    case CHECK_OK:
        return "OK";
    case CHECK_WARN:
        return "WARN";
    case CHECK_FAIL:
        return "FAIL";
    case CHECK_SKIP:
        return "SKIP";
    }
    return "?";
}

const char *status_json(check_status_t status) {
    switch (status) {
    case CHECK_OK:
        return "ok";
    case CHECK_WARN:
        return "warn";
    case CHECK_FAIL:
        return "fail";
    case CHECK_SKIP:
        return "skip";
    }
    return "unknown";
}

const char *status_symbol(check_status_t status) {
    switch (status) {
    case CHECK_OK:
        return "✓"; /* check mark */
    case CHECK_WARN:
        return "!";
    case CHECK_FAIL:
        return "✗"; /* ballot x */
    case CHECK_SKIP:
        return "-";
    }
    return "?";
}

int run_all_checks(check_result_list_t *results) {
    if (run_system_checks(results) != 0) {
        return -1;
    }
    if (run_service_checks(results) != 0) {
        return -1;
    }
    if (run_package_checks(results) != 0) {
        return -1;
    }
    if (run_desktop_checks(results) != 0) {
        return -1;
    }
    return 0;
}
