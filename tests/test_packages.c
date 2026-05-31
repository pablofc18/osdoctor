/*
 * test_packages.c - unit tests for the backend-agnostic Packages logic: the
 * bounded detail accumulator (pkg_detail_append) and the query-status ->
 * check-result classifier (pkg_emit). The actual libalpm / pacman I/O is
 * exercised at the integration level by the scan smoke test.
 */
#include "check_packages_internal.h"
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

/* ---- pkg_detail_append ---- */

static void test_detail_append_basic(void) {
    char detail[64];
    detail[0] = '\0';
    size_t used = 0;

    pkg_detail_append(detail, sizeof detail, &used, "alpha");
    CHECK_STR(detail, "alpha");
    CHECK(used == 5);

    pkg_detail_append(detail, sizeof detail, &used, "beta");
    CHECK_STR(detail, "alpha, beta");

    pkg_detail_append(detail, sizeof detail, &used, "gamma");
    CHECK_STR(detail, "alpha, beta, gamma");
    CHECK(used == strlen(detail));
}

static void test_detail_append_null_inputs(void) {
    char detail[16];
    detail[0] = '\0';
    size_t used = 0;

    pkg_detail_append(detail, sizeof detail, &used, NULL);
    CHECK_STR(detail, "");
    CHECK(used == 0);

    pkg_detail_append(NULL, 0, &used, "x");
    CHECK(used == 0);
}

static void test_detail_append_truncation(void) {
    char detail[8];
    detail[0] = '\0';
    size_t used = 0;

    pkg_detail_append(detail, sizeof detail, &used, "aaa");
    CHECK_STR(detail, "aaa");

    pkg_detail_append(detail, sizeof detail, &used, "bbbbbb");
    CHECK(strlen(detail) < sizeof detail);
    CHECK(used <= sizeof detail - 1);

    size_t before = used;
    pkg_detail_append(detail, sizeof detail, &used, "ccc");
    CHECK(used == before);
}

/* ---- pkg_emit ---- */

static const check_result_t *last(const check_result_list_t *r) {
    return &r->items[r->len - 1];
}

static void test_emit_ok_zero(void) {
    check_result_list_t r;
    results_init(&r);

    CHECK(pkg_emit(&r, PKG_QUERY_OK, 0, "") == 0);
    CHECK(last(&r)->status == CHECK_OK);
    CHECK_STR(last(&r)->id, "orphan-packages");
    CHECK_STR(last(&r)->group, "Packages");
    CHECK_STR(last(&r)->message, "No orphan packages");

    results_free(&r);
}

static void test_emit_ok_counts(void) {
    check_result_list_t r;
    results_init(&r);

    CHECK(pkg_emit(&r, PKG_QUERY_OK, 3, "a, b, c") == 0);
    CHECK(last(&r)->status == CHECK_WARN);
    CHECK_STR(last(&r)->message, "3 orphan packages detected");
    CHECK_STR(last(&r)->detail, "a, b, c");

    /* Singular wording at exactly one orphan. */
    CHECK(pkg_emit(&r, PKG_QUERY_OK, 1, "only") == 0);
    CHECK_STR(last(&r)->message, "1 orphan package detected");

    results_free(&r);
}

static void test_emit_unavailable(void) {
    check_result_list_t r;
    results_init(&r);

    CHECK(pkg_emit(&r, PKG_QUERY_UNAVAILABLE, 0, "") == 0);
    CHECK(last(&r)->status == CHECK_SKIP);
    CHECK_STR(last(&r)->message, "pacman not available");

    results_free(&r);
}

int main(void) {
    printf("test_packages\n");
    test_detail_append_basic();
    test_detail_append_null_inputs();
    test_detail_append_truncation();
    test_emit_ok_zero();
    test_emit_ok_counts();
    test_emit_unavailable();
    printf("  %d checks, %d failures\n", g_checks, g_failures);
    return (g_failures == 0) ? 0 : 1;
}
