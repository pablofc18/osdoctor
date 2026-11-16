/*
 * test_output.c - unit tests for the result model (status strings, summary,
 * exit codes, overall status) and the text/JSON renderers, including golden
 * tests that render the canonical sample report and compare it byte-for-byte
 * with examples/sample-output.{txt,json}. The Rust implementation checks the
 * same sample against the same files, so both stay in lockstep.
 *
 * Run from the repository root (as `make test` does) so the examples/ paths
 * resolve.
 */
#include "osdoctor.h"
#include "output.h"
#include "util_fs.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

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
            printf("  FAIL: strings differ (line %d)\n--- got ---\n%s\n--- want ---\n%s\n",        \
                   __LINE__, (a), (b));                                                            \
        }                                                                                          \
    } while (0)

typedef void (*renderer_t)(const check_result_list_t *);

/*
 * Run `render` with stdout redirected to a temporary file and return what it
 * wrote (caller frees). stdout is then not a TTY, so the text renderer emits
 * the plain (uncolored) form, matching the committed samples.
 */
static char *capture(renderer_t render, const check_result_list_t *r) {
    fflush(stdout);
    FILE *tmp = tmpfile();
    if (tmp == NULL) {
        return NULL;
    }
    int saved = dup(STDOUT_FILENO);
    dup2(fileno(tmp), STDOUT_FILENO);
    render(r);
    fflush(stdout);
    dup2(saved, STDOUT_FILENO);
    close(saved);

    long len = ftell(tmp);
    char *buf = malloc((size_t)(len > 0 ? len : 0) + 1);
    if (buf != NULL) {
        rewind(tmp);
        size_t n = fread(buf, 1, (size_t)(len > 0 ? len : 0), tmp);
        buf[n] = '\0';
    }
    fclose(tmp);
    return buf;
}

/* The canonical sample report. Kept in sync with examples/sample-output.* and
 * with sample_results() in the Rust tests. */
static void sample_results(check_result_list_t *r) {
    results_add(r, "kernel-version", "System", CHECK_OK, "Kernel detected: 7.0.9-arch2-1",
                "Linux 7.0.9-arch2-1 #1 SMP PREEMPT_DYNAMIC Fri, 22 May 2026 19:25:09 +0000");
    results_add(r, "root-disk-space", "System", CHECK_OK, "Root filesystem has 403 GB free", "");
    results_add(r, "tmp-writable", "System", CHECK_OK, "/tmp is writable", "");
    results_add(r, "memory-info", "System", CHECK_OK, "Memory: 26792 MB available of 31956 MB", "");
    results_add(r, "failed-system-services", "Services", CHECK_OK, "No failed system services", "");
    results_add(r, "failed-user-services", "Services", CHECK_OK, "No failed user services", "");
    results_add(r, "pacman-lock", "Packages", CHECK_OK, "pacman database is unlocked", "");
    results_add(r, "pacman-log", "Packages", CHECK_OK, "pacman log exists", "");
    results_add(r, "orphan-packages", "Packages", CHECK_OK, "No orphan packages", "");
    results_add(r, "wayland-session", "Desktop", CHECK_OK, "Wayland session detected", "");
    results_add(r, "hyprland-socket", "Desktop", CHECK_OK, "Hyprland socket detected", "");
    results_add(r, "hyprland-binds", "Desktop", CHECK_WARN,
                "Hyprland bind references missing command: ghostty", "");
    results_add(r, "waybar-scripts", "Desktop", CHECK_FAIL,
                "Waybar config references missing script: ~/.config/waybar/scripts/battery", "");
}

/* ---- model ---- */

static void test_status_strings(void) {
    CHECK_STR(status_label(CHECK_OK), "OK");
    CHECK_STR(status_json(CHECK_WARN), "warn");
    CHECK_STR(status_symbol(CHECK_FAIL), "✗");
    CHECK_STR(status_label(CHECK_SKIP), "SKIP");
}

static void test_summarize_counts(void) {
    check_result_list_t r;
    results_init(&r);
    results_add(&r, "id", "G", CHECK_OK, "m", "");
    results_add(&r, "id", "G", CHECK_OK, "m", "");
    results_add(&r, "id", "G", CHECK_WARN, "m", "");
    results_add(&r, "id", "G", CHECK_FAIL, "m", "");
    results_add(&r, "id", "G", CHECK_SKIP, "m", "");
    check_summary_t s = summarize_results(&r);
    CHECK(s.ok == 2 && s.warn == 1 && s.fail == 1 && s.skip == 1);
    results_free(&r);
}

static void test_exit_codes(void) {
    check_summary_t fail = {.fail = 1};
    check_summary_t warn = {.warn = 1};
    check_summary_t clean = {.ok = 3};
    CHECK(exit_code_from_summary(fail, false) == 2);
    CHECK(exit_code_from_summary(fail, true) == 2);
    CHECK(exit_code_from_summary(warn, false) == 1);
    CHECK(exit_code_from_summary(warn, true) == 2);
    CHECK(exit_code_from_summary(clean, false) == 0);
}

static void test_overall(void) {
    CHECK(overall_status((check_summary_t){.fail = 1}) == CHECK_FAIL);
    CHECK(overall_status((check_summary_t){.warn = 1}) == CHECK_WARN);
    CHECK(overall_status((check_summary_t){0}) == CHECK_OK);
}

/* ---- JSON renderer ---- */

static void test_json_empty(void) {
    check_result_list_t r;
    results_init(&r);
    char *out = capture(output_json, &r);
    CHECK(out != NULL);
    if (out != NULL) {
        CHECK_STR(out, "{\n  \"status\": \"ok\",\n  \"summary\": {\n    \"ok\": 0,\n    \"warn\": "
                       "0,\n    \"fail\": 0,\n    \"skip\": 0\n  },\n  \"groups\": [\n  ]\n}\n");
    }
    free(out);
    results_free(&r);
}

static void test_json_one_group_two_checks_with_detail(void) {
    check_result_list_t r;
    results_init(&r);
    results_add(&r, "a-id", "System", CHECK_OK, "msg a", "det a");
    results_add(&r, "b-id", "System", CHECK_WARN, "msg b", "");
    char *out = capture(output_json, &r);
    CHECK(out != NULL);
    if (out != NULL) {
        CHECK_STR(out,
                  "{\n  \"status\": \"warn\",\n  \"summary\": {\n    \"ok\": 1,\n    \"warn\": "
                  "1,\n    \"fail\": 0,\n    \"skip\": 0\n  },\n  \"groups\": [\n    {\n      "
                  "\"name\": \"System\",\n      \"checks\": [\n        {\n          \"id\": "
                  "\"a-id\",\n          \"status\": \"ok\",\n          \"message\": \"msg "
                  "a\",\n          \"detail\": \"det a\"\n        },\n        {\n          "
                  "\"id\": \"b-id\",\n          \"status\": \"warn\",\n          \"message\": "
                  "\"msg b\"\n        }\n      ]\n    }\n  ]\n}\n");
    }
    free(out);
    results_free(&r);
}

/* ---- text renderer ---- */

static void test_text_groups_and_summary(void) {
    check_result_list_t r;
    results_init(&r);
    results_add(&r, "k", "System", CHECK_OK, "Kernel ok", "Linux x");
    results_add(&r, "w", "Desktop", CHECK_WARN, "missing thing", "");
    char *out = capture(output_text, &r);
    CHECK(out != NULL);
    if (out != NULL) {
        CHECK_STR(out, "System\n  ✓ Kernel ok\n      Linux x\n\nDesktop\n  ! missing "
                       "thing\n\nSummary\n  Status: WARN\n  Checks: 1 ok, 1 warning, 0 failures\n");
    }
    free(out);
    results_free(&r);
}

static void test_text_skipped_count_appended(void) {
    check_result_list_t r;
    results_init(&r);
    results_add(&r, "s", "System", CHECK_SKIP, "skipped", "");
    char *out = capture(output_text, &r);
    CHECK(out != NULL && strstr(out, "  Checks: 0 ok, 0 warnings, 0 failures, 1 skipped\n"));
    free(out);
    results_free(&r);
}

/* ---- golden samples ---- */

static void check_golden(renderer_t render, const char *path) {
    check_result_list_t r;
    results_init(&r);
    sample_results(&r);
    char *out = capture(render, &r);
    char *want = fs_read_file(path, 1 << 20, NULL);
    CHECK(out != NULL);
    CHECK(want != NULL);
    if (out != NULL && want != NULL) {
        CHECK_STR(out, want);
    }
    free(out);
    free(want);
    results_free(&r);
}

static void test_golden_json(void) {
    check_golden(output_json, "examples/sample-output.json");
}

static void test_golden_text(void) {
    check_golden(output_text, "examples/sample-output.txt");
}

int main(void) {
    printf("test_output\n");
    test_status_strings();
    test_summarize_counts();
    test_exit_codes();
    test_overall();
    test_json_empty();
    test_json_one_group_two_checks_with_detail();
    test_text_groups_and_summary();
    test_text_skipped_count_appended();
    test_golden_json();
    test_golden_text();
    printf("  %d checks, %d failures\n", g_checks, g_failures);
    return (g_failures == 0) ? 0 : 1;
}
