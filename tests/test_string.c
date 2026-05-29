/*
 * test_string.c - unit tests for the string helpers.
 *
 * No framework: a couple of macros that print PASS/FAIL and track a global
 * failure count. The process exits non-zero if any assertion failed.
 */
#include "util_string.h"

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

static void test_trim(void) {
    char a[] = "  hello  ";
    CHECK_STR(str_trim(a), "hello");
    char b[] = "no-trim";
    CHECK_STR(str_trim(b), "no-trim");
    char c[] = "   ";
    CHECK_STR(str_trim(c), "");
    char d[] = "";
    CHECK_STR(str_trim(d), "");
    char e[] = "\t spaced\n";
    CHECK_STR(str_trim(e), "spaced");
}

static void test_starts_with(void) {
    CHECK(str_starts_with("foobar", "foo"));
    CHECK(!str_starts_with("foo", "foobar"));
    CHECK(str_starts_with("anything", ""));
    CHECK(str_starts_with("bind = X", "bind"));
    CHECK(!str_starts_with("", "x"));
    CHECK(!str_starts_with(NULL, "x"));
}

static void test_contains(void) {
    CHECK(str_contains("hello world", "lo w"));
    CHECK(!str_contains("hello", "xyz"));
    CHECK(str_contains("/home/x/.config/y", "/.config/"));
    CHECK(!str_contains(NULL, "x"));
}

static void test_split_lines(void) {
    size_t n = 0;
    char **lines = str_split_lines("a\nb\nc", &n);
    CHECK(n == 3);
    if (n == 3) {
        CHECK_STR(lines[0], "a");
        CHECK_STR(lines[1], "b");
        CHECK_STR(lines[2], "c");
    }
    str_free_lines(lines, n);

    lines = str_split_lines("only\n", &n); /* trailing newline -> 1 line */
    CHECK(n == 1);
    if (n == 1) {
        CHECK_STR(lines[0], "only");
    }
    str_free_lines(lines, n);

    lines = str_split_lines("crlf\r\nsecond\r\n", &n); /* CR stripped */
    CHECK(n == 2);
    if (n == 2) {
        CHECK_STR(lines[0], "crlf");
        CHECK_STR(lines[1], "second");
    }
    str_free_lines(lines, n);

    lines = str_split_lines("", &n);
    CHECK(n == 0);
    str_free_lines(lines, n);
}

static void test_json_escape(void) {
    char out[256];

    CHECK(json_escape("plain", out, sizeof out) == 0);
    CHECK_STR(out, "plain");

    CHECK(json_escape("a\"b\\c", out, sizeof out) == 0);
    CHECK_STR(out, "a\\\"b\\\\c");

    CHECK(json_escape("tab\tnl\n", out, sizeof out) == 0);
    CHECK_STR(out, "tab\\tnl\\n");

    char ctrl[2] = {0x01, 0x00};
    CHECK(json_escape(ctrl, out, sizeof out) == 0);
    CHECK_STR(out, "\\u0001");

    /* Truncation: buffer too small must return -1 and stay NUL-terminated. */
    char small[4];
    CHECK(json_escape("abcdef", small, sizeof small) == -1);
    CHECK(strlen(small) < sizeof small);
}

int main(void) {
    printf("test_string\n");
    test_trim();
    test_starts_with();
    test_contains();
    test_split_lines();
    test_json_escape();
    printf("  %d checks, %d failures\n", g_checks, g_failures);
    return (g_failures == 0) ? 0 : 1;
}
