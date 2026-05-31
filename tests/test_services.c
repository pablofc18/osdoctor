/*
 * test_services.c - unit tests for the backend-agnostic Services logic:
 * the bounded detail accumulator (svc_detail_append) and the query-status ->
 * check-result classifier (svc_emit). The actual systemctl / sd-bus I/O is
 * exercised at the integration level by the scan smoke test.
 */
#include "check_services_internal.h"
#include "osdoctor.h"

#include <stdio.h>
#include <string.h>

static int g_failures = 0;
static int g_checks = 0;

#define CHECK(cond)                                                                                \
    do {                                                                                           \
        g_checks++;                                                                                \
        if (!(cond)) {                                                                             \
            g_failures++;                                                                          \
            printf("  FAIL: %s (line %d)\n", #cond, __LINE__);                                     \
        }                                                                                          \
    } while (0)

#define CHECK_STR(a, b)                                                                            \
    do {                                                                                           \
        g_checks++;                                                                                \
        if (strcmp((a), (b)) != 0) {                                                               \
            g_failures++;                                                                          \
            printf("  FAIL: \"%s\" != \"%s\" (line %d)\n", (a), (b), __LINE__);                    \
        }                                                                                          \
    } while (0)

/* ---- svc_detail_append ---- */

static void test_detail_append_basic(void) {
    char detail[64];
    detail[0] = '\0';
    size_t used = 0;

    svc_detail_append(detail, sizeof detail, &used, "alpha");
    CHECK_STR(detail, "alpha");
    CHECK(used == 5);

    svc_detail_append(detail, sizeof detail, &used, "beta");
    CHECK_STR(detail, "alpha, beta");

    svc_detail_append(detail, sizeof detail, &used, "gamma");
    CHECK_STR(detail, "alpha, beta, gamma");
    CHECK(used == strlen(detail));
}

static void test_detail_append_null_inputs(void) {
    char detail[16];
    detail[0] = '\0';
    size_t used = 0;

    /* NULL unit and NULL detail must be safe no-ops. */
    svc_detail_append(detail, sizeof detail, &used, NULL);
    CHECK_STR(detail, "");
    CHECK(used == 0);

    svc_detail_append(NULL, 0, &used, "x");
    CHECK(used == 0);
}

static void test_detail_append_truncation(void) {
    /* Buffer too small to hold the second name: stay NUL-terminated, do not
     * overflow, and clamp `used` to size-1 so later appends are no-ops. */
    char detail[8];
    detail[0] = '\0';
    size_t used = 0;

    svc_detail_append(detail, sizeof detail, &used, "aaa"); /* "aaa" */
    CHECK_STR(detail, "aaa");

    svc_detail_append(detail, sizeof detail, &used, "bbbbbb"); /* would overflow */
    CHECK(strlen(detail) < sizeof detail);
    CHECK(used <= sizeof detail - 1);

    size_t before = used;
    svc_detail_append(detail, sizeof detail, &used, "ccc"); /* no room left */
    CHECK(used == before);
}

/* ---- svc_emit ---- */

static const check_result_t *last(const check_result_list_t *r) {
    return &r->items[r->len - 1];
}

static void test_emit_ok_zero(void) {
    check_result_list_t r;
    results_init(&r);

    CHECK(svc_emit(&r, SVC_SCOPE_SYSTEM, SVC_QUERY_OK, 0, "") == 0);
    CHECK(last(&r)->status == CHECK_OK);
    CHECK_STR(last(&r)->id, "failed-system-services");
    CHECK_STR(last(&r)->group, "Services");
    CHECK_STR(last(&r)->message, "No failed system services");

    CHECK(svc_emit(&r, SVC_SCOPE_USER, SVC_QUERY_OK, 0, "") == 0);
    CHECK(last(&r)->status == CHECK_OK);
    CHECK_STR(last(&r)->id, "failed-user-services");
    CHECK_STR(last(&r)->message, "No failed user services");

    results_free(&r);
}

static void test_emit_ok_failed_counts(void) {
    check_result_list_t r;
    results_init(&r);

    CHECK(svc_emit(&r, SVC_SCOPE_SYSTEM, SVC_QUERY_OK, 3, "a, b, c") == 0);
    CHECK(last(&r)->status == CHECK_WARN);
    CHECK_STR(last(&r)->message, "3 failed system services");
    CHECK_STR(last(&r)->detail, "a, b, c");

    /* Singular wording at exactly one failed unit. */
    CHECK(svc_emit(&r, SVC_SCOPE_SYSTEM, SVC_QUERY_OK, 1, "only") == 0);
    CHECK_STR(last(&r)->message, "1 failed system service");

    CHECK(svc_emit(&r, SVC_SCOPE_USER, SVC_QUERY_OK, 1, "only") == 0);
    CHECK_STR(last(&r)->message, "1 failed user service");

    results_free(&r);
}

static void test_emit_no_session(void) {
    check_result_list_t r;
    results_init(&r);

    CHECK(svc_emit(&r, SVC_SCOPE_USER, SVC_QUERY_NO_SESSION, 0, "") == 0);
    CHECK(last(&r)->status == CHECK_SKIP);
    CHECK_STR(last(&r)->message, "No systemd user session available");

    results_free(&r);
}

static void test_emit_unavailable(void) {
    check_result_list_t r;
    results_init(&r);

    CHECK(svc_emit(&r, SVC_SCOPE_SYSTEM, SVC_QUERY_UNAVAILABLE, 0, "") == 0);
    CHECK(last(&r)->status == CHECK_SKIP);
    CHECK_STR(last(&r)->message, "systemctl not available");

    CHECK(svc_emit(&r, SVC_SCOPE_USER, SVC_QUERY_UNAVAILABLE, 0, "") == 0);
    CHECK(last(&r)->status == CHECK_SKIP);
    CHECK_STR(last(&r)->message, "systemctl not available");

    results_free(&r);
}

int main(void) {
    printf("test_services\n");
    test_detail_append_basic();
    test_detail_append_null_inputs();
    test_detail_append_truncation();
    test_emit_ok_zero();
    test_emit_ok_failed_counts();
    test_emit_no_session();
    test_emit_unavailable();
    printf("  %d checks, %d failures\n", g_checks, g_failures);
    return (g_failures == 0) ? 0 : 1;
}
