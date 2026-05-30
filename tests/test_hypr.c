/*
 * test_hypr.c - unit tests for Hyprland `source =` include resolution and
 * exec-bind parsing (hypr_scan_binds).
 *
 * Each test builds a self-contained config tree under a private mkdtemp()
 * directory, so the tests do not depend on the real ~/.config/hypr layout.
 * The resolvable command used is "sh" (reliably in PATH); a name that cannot
 * exist is used to prove a given file was actually parsed.
 */
#include "check_desktop_internal.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
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

/* ---- helpers ---- */

static void write_file(const char *path, const char *content) {
    FILE *f = fopen(path, "w");
    if (f != NULL) {
        fputs(content, f);
        fclose(f);
    }
}

static void make_tmpdir(char *out, size_t out_size) {
    char tmpl[] = "/tmp/osdoctor_hypr_XXXXXX";
    char *dir = mkdtemp(tmpl);
    snprintf(out, out_size, "%s", dir != NULL ? dir : ".");
}

static bool has_missing(const hypr_bind_scan_t *s, const char *cmd) {
    for (size_t i = 0; i < s->missing.len; i++) {
        if (strcmp(s->missing.items[i], cmd) == 0) {
            return true;
        }
    }
    return false;
}

/* ---- tests ---- */

static void test_root_binds(void) {
    char dir[256];
    make_tmpdir(dir, sizeof dir);
    char root[512];
    snprintf(root, sizeof root, "%s/hyprland.conf", dir);
    write_file(root, "# comment\n"
                     "bind = SUPER, Return, exec, sh\n"
                     "bind = SUPER, X, exec, osd_bogus_root\n");

    hypr_bind_scan_t scan = {0};
    hypr_scan_binds(root, &scan);

    CHECK(scan.checked == 2);
    CHECK(has_missing(&scan, "osd_bogus_root"));
    CHECK(!has_missing(&scan, "sh"));
    CHECK(scan.missing.len == 1);
}

static void test_relative_include(void) {
    char dir[256];
    make_tmpdir(dir, sizeof dir);
    char root[512], inc[512];
    snprintf(root, sizeof root, "%s/hyprland.conf", dir);
    snprintf(inc, sizeof inc, "%s/binds.conf", dir);
    write_file(root, "source = binds.conf\n");
    write_file(inc, "bind = SUPER, Return, exec, sh\n"
                    "bind = SUPER, X, exec, osd_bogus_inc\n");

    hypr_bind_scan_t scan = {0};
    hypr_scan_binds(root, &scan);

    CHECK(scan.checked == 2);
    CHECK(has_missing(&scan, "osd_bogus_inc"));
    CHECK(scan.missing.len == 1);
}

static void test_tilde_include(void) {
    char dir[256];
    make_tmpdir(dir, sizeof dir);
    setenv("HOME", dir, 1);
    char subdir[512];
    snprintf(subdir, sizeof subdir, "%s/inc", dir);
    mkdir(subdir, 0700);

    char root[512], inc[600];
    snprintf(root, sizeof root, "%s/hyprland.conf", dir);
    snprintf(inc, sizeof inc, "%s/sub.conf", subdir);
    write_file(root, "source = ~/inc/sub.conf\n");
    write_file(inc, "bind = SUPER, A, exec, osd_bogus_tilde\n");

    hypr_bind_scan_t scan = {0};
    hypr_scan_binds(root, &scan);

    CHECK(scan.checked == 1);
    CHECK(has_missing(&scan, "osd_bogus_tilde"));
}

static void test_envvar_include(void) {
    char dir[256];
    make_tmpdir(dir, sizeof dir);
    setenv("OSD_TEST_ENVDIR", dir, 1);

    char root[512], a[512], b[512];
    snprintf(root, sizeof root, "%s/hyprland.conf", dir);
    snprintf(a, sizeof a, "%s/env.conf", dir);
    snprintf(b, sizeof b, "%s/env2.conf", dir);
    write_file(root, "source = $OSD_TEST_ENVDIR/env.conf\n"
                     "source = ${OSD_TEST_ENVDIR}/env2.conf\n");
    write_file(a, "bind = SUPER, B, exec, osd_bogus_env1\n");
    write_file(b, "bind = SUPER, C, exec, osd_bogus_env2\n");

    hypr_bind_scan_t scan = {0};
    hypr_scan_binds(root, &scan);

    CHECK(scan.checked == 2);
    CHECK(has_missing(&scan, "osd_bogus_env1"));
    CHECK(has_missing(&scan, "osd_bogus_env2"));
}

static void test_glob_include(void) {
    char dir[256];
    make_tmpdir(dir, sizeof dir);
    char parts[512];
    snprintf(parts, sizeof parts, "%s/parts", dir);
    mkdir(parts, 0700);

    char root[512], a[600], b[600];
    snprintf(root, sizeof root, "%s/hyprland.conf", dir);
    snprintf(a, sizeof a, "%s/a.conf", parts);
    snprintf(b, sizeof b, "%s/b.conf", parts);
    write_file(root, "source = parts/*.conf\n");
    write_file(a, "bind = SUPER, A, exec, osd_bogus_glob_a\n");
    write_file(b, "bind = SUPER, B, exec, osd_bogus_glob_b\n");

    hypr_bind_scan_t scan = {0};
    hypr_scan_binds(root, &scan);

    CHECK(scan.checked == 2);
    CHECK(has_missing(&scan, "osd_bogus_glob_a"));
    CHECK(has_missing(&scan, "osd_bogus_glob_b"));
}

static void test_cycle_terminates(void) {
    char dir[256];
    make_tmpdir(dir, sizeof dir);
    char a[512], b[512];
    snprintf(a, sizeof a, "%s/a.conf", dir);
    snprintf(b, sizeof b, "%s/b.conf", dir);
    write_file(a, "source = b.conf\n"
                  "bind = SUPER, A, exec, osd_bogus_cyc_a\n");
    write_file(b, "source = a.conf\n"
                  "bind = SUPER, B, exec, osd_bogus_cyc_b\n");

    hypr_bind_scan_t scan = {0};
    hypr_scan_binds(a, &scan); /* must terminate */

    CHECK(scan.checked == 2);
    CHECK(has_missing(&scan, "osd_bogus_cyc_a"));
    CHECK(has_missing(&scan, "osd_bogus_cyc_b"));
}

static void test_depth_guard(void) {
    char dir[256];
    make_tmpdir(dir, sizeof dir);
    const int chain = MAX_INCLUDE_DEPTH + 5;
    for (int i = 0; i < chain; i++) {
        char path[512];
        snprintf(path, sizeof path, "%s/f%d.conf", dir, i);
        char content[256];
        if (i + 1 < chain) {
            snprintf(content, sizeof content,
                     "source = f%d.conf\nbind = SUPER, A, exec, osd_bogus_d%d\n", i + 1, i);
        } else {
            snprintf(content, sizeof content, "bind = SUPER, A, exec, osd_bogus_d%d\n", i);
        }
        write_file(path, content);
    }
    char root[512];
    snprintf(root, sizeof root, "%s/f0.conf", dir);

    hypr_bind_scan_t scan = {0};
    hypr_scan_binds(root, &scan); /* must terminate, bounded by depth */

    /* Files at depth 0..MAX_INCLUDE_DEPTH inclusive are processed. */
    CHECK(scan.checked == MAX_INCLUDE_DEPTH + 1);
}

static void test_bindd_description_field(void) {
    /* `bindd` (and any flag set containing 'd') inserts a description field
     * between the key and the dispatcher: mods, key, description, dispatcher,
     * args. The command must still be found. */
    char dir[256];
    make_tmpdir(dir, sizeof dir);
    char root[512];
    snprintf(root, sizeof root, "%s/hyprland.conf", dir);
    write_file(root, "bindd = SUPER, Return, Terminal, exec, sh\n"
                     "bindd = SUPER, X, Some App, exec, osd_bogus_bindd\n"
                     "bindde = SUPER, Y, Repeating, exec, osd_bogus_bindde\n");

    hypr_bind_scan_t scan = {0};
    hypr_scan_binds(root, &scan);

    CHECK(scan.checked == 3);
    CHECK(has_missing(&scan, "osd_bogus_bindd"));
    CHECK(has_missing(&scan, "osd_bogus_bindde"));
    CHECK(!has_missing(&scan, "sh"));
}

static void test_missing_source_skipped(void) {
    char dir[256];
    make_tmpdir(dir, sizeof dir);
    char root[512];
    snprintf(root, sizeof root, "%s/hyprland.conf", dir);
    write_file(root, "source = does_not_exist.conf\n"
                     "bind = SUPER, Z, exec, osd_bogus_after\n");

    hypr_bind_scan_t scan = {0};
    hypr_scan_binds(root, &scan);

    CHECK(scan.checked == 1);
    CHECK(has_missing(&scan, "osd_bogus_after"));
}

int main(void) {
    printf("test_hypr\n");
    test_root_binds();
    test_relative_include();
    test_tilde_include();
    test_envvar_include();
    test_glob_include();
    test_cycle_terminates();
    test_depth_guard();
    test_bindd_description_field();
    test_missing_source_skipped();
    printf("  %d checks, %d failures\n", g_checks, g_failures);
    return (g_failures == 0) ? 0 : 1;
}
