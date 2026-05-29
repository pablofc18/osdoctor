/*
 * test_fs.c - unit tests for the filesystem helpers.
 *
 * Uses a private temp directory (mkdtemp) so the tests are self-contained and
 * do not depend on the surrounding environment beyond a POSIX /bin/sh.
 */
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
            printf("  FAIL: \"%s\" != \"%s\" (line %d)\n", (a), (b), __LINE__);                    \
        }                                                                                          \
    } while (0)

static void test_join_path(void) {
    char out[256];
    CHECK(join_path("a", "b", out, sizeof out) == 0);
    CHECK_STR(out, "a/b");

    CHECK(join_path("a/", "b", out, sizeof out) == 0);
    CHECK_STR(out, "a/b");

    CHECK(join_path("/usr", "/bin", out, sizeof out) == 0);
    CHECK_STR(out, "/usr/bin");

    char tiny[4];
    CHECK(join_path("longdir", "longfile", tiny, sizeof tiny) == -1);
}

static void test_expand_home(void) {
    setenv("HOME", "/home/tester", 1);

    char *a = expand_home_path("~/x");
    CHECK(a != NULL);
    if (a != NULL) {
        CHECK_STR(a, "/home/tester/x");
    }
    free(a);

    char *b = expand_home_path("~");
    CHECK(b != NULL);
    if (b != NULL) {
        CHECK_STR(b, "/home/tester");
    }
    free(b);

    char *c = expand_home_path("/absolute/path");
    CHECK(c != NULL);
    if (c != NULL) {
        CHECK_STR(c, "/absolute/path");
    }
    free(c);

    /* A leading "~user" is NOT expanded (only bare ~ / ~/). */
    char *d = expand_home_path("~root/x");
    CHECK(d != NULL);
    if (d != NULL) {
        CHECK_STR(d, "~root/x");
    }
    free(d);
}

static void test_executable_in_path(void) {
    /* /bin/sh is reliably present and executable on any POSIX system. */
    CHECK(is_executable_in_path("sh"));
    CHECK(is_executable_in_path("/bin/sh"));
    CHECK(!is_executable_in_path("osdoctor_definitely_not_a_real_command_xyz"));
    CHECK(!is_executable_in_path("/no/such/path/binary"));
}

static void test_file_predicates(void) {
    char tmpl[] = "/tmp/osdoctor_test_XXXXXX";
    char *dir = mkdtemp(tmpl);
    CHECK(dir != NULL);
    if (dir == NULL) {
        return;
    }

    char file[512];
    snprintf(file, sizeof file, "%s/data.txt", dir);

    CHECK(!file_exists(file));

    FILE *fp = fopen(file, "w");
    CHECK(fp != NULL);
    if (fp != NULL) {
        fputs("hello from osdoctor\n", fp);
        fclose(fp);
    }

    CHECK(file_exists(file));
    CHECK(file_readable(file));
    CHECK(path_writable(dir));

    size_t len = 0;
    char *content = fs_read_file(file, 1024, &len);
    CHECK(content != NULL);
    if (content != NULL) {
        CHECK(len == strlen("hello from osdoctor\n"));
        CHECK_STR(content, "hello from osdoctor\n");
        free(content);
    }

    /* Clean up. */
    unlink(file);
    rmdir(dir);
}

int main(void) {
    printf("test_fs\n");
    test_join_path();
    test_expand_home();
    test_executable_in_path();
    test_file_predicates();
    printf("  %d checks, %d failures\n", g_checks, g_failures);
    return (g_failures == 0) ? 0 : 1;
}
