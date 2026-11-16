/*
 * test_waybar.c - unit tests for the Waybar script scanner
 * (waybar_scan_scripts): which tokens count as local scripts, relative
 * resolution against the config directory, and JSONC comment handling.
 */
#include "check_desktop_internal.h"

#include <stdbool.h>
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

static bool has_missing(const strset_t *s, const char *tok) {
    for (size_t i = 0; i < s->len; i++) {
        if (strcmp(s->items[i], tok) == 0) {
            return true;
        }
    }
    return false;
}

static void test_scan_relative_scripts(void) {
    char tmpl[] = "/tmp/osdoctor_waybar_XXXXXX";
    char *dir = mkdtemp(tmpl);
    CHECK(dir != NULL);
    if (dir == NULL) {
        return;
    }
    char scripts[512], present[600];
    snprintf(scripts, sizeof scripts, "%s/scripts", dir);
    mkdir(scripts, 0700);
    snprintf(present, sizeof present, "%s/present", scripts);
    FILE *f = fopen(present, "w");
    if (f != NULL) {
        fputs("#!/bin/sh\n", f);
        fclose(f);
    }

    /* Two referenced scripts: one present, one missing. */
    strset_t missing = {0};
    int checked = 0;
    waybar_scan_scripts("{ \"custom/a\": { \"exec\": \"scripts/present\" }, "
                        "\"custom/b\": { \"exec\": \"scripts/missing\" } }",
                        dir, &missing, &checked);
    CHECK(checked == 2);
    CHECK(missing.len == 1);
    CHECK(has_missing(&missing, "scripts/missing"));

    unlink(present);
    rmdir(scripts);
    rmdir(dir);
}

static void test_urls_are_not_scripts(void) {
    /* URLs are opened, not executed from disk. */
    strset_t missing = {0};
    int checked = 0;
    waybar_scan_scripts("{ \"on-click\": \"xdg-open https://example.com/scripts/x\", "
                        "\"b\": \"file:///home/u/.config/waybar/scripts/x\" }",
                        "/nonexistent", &missing, &checked);
    CHECK(checked == 0);
    CHECK(missing.len == 0);
}

static void test_jsonc_comments_ignored(void) {
    /* Waybar configs are JSONC: script paths inside // and block comments are
     * not referenced. Comment markers inside strings are kept. */
    strset_t missing = {0};
    int checked = 0;
    waybar_scan_scripts("// \"scripts/line-comment\"\n"
                        "{\n"
                        "  /* \"scripts/block-comment\"\n"
                        "     still \"scripts/block-comment-2\" */\n"
                        "  \"exec\": \"scripts/real\", // \"scripts/trailing\"\n"
                        "  \"c\": \"scripts/kept // not-a-comment\", /**/ "
                        "\"b\": \"scripts/after-empty-block\"\n"
                        "}\n"
                        "/* unterminated \"scripts/unterminated\"",
                        "/nonexistent", &missing, &checked);
    CHECK(checked == 3);
    CHECK(missing.len == 3);
    CHECK(has_missing(&missing, "scripts/real"));
    CHECK(has_missing(&missing, "scripts/kept"));
    CHECK(has_missing(&missing, "scripts/after-empty-block"));
}

int main(void) {
    printf("test_waybar\n");
    test_scan_relative_scripts();
    test_urls_are_not_scripts();
    test_jsonc_comments_ignored();
    printf("  %d checks, %d failures\n", g_checks, g_failures);
    return (g_failures == 0) ? 0 : 1;
}
