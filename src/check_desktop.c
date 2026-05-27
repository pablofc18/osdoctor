/*
 * check_desktop.c - Wayland / Hyprland / Waybar session checks.
 *
 * The config parsers here are deliberately conservative: they look for the
 * common, simple cases and skip anything that looks like a shell expression,
 * a variable, or otherwise ambiguous input. The goal is "no false alarms"
 * rather than "perfect coverage".
 */
#include "checks.h"
#include "osdoctor.h"
#include "util_fs.h"
#include "util_string.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const char *GROUP = "Desktop";

#define CONFIG_MAX_BYTES (1u << 20) /* 1 MiB cap when reading config files */

/* ---- tiny de-duplicating string set ---- */

#define SET_CAP 64
#define SET_ITEM 256

typedef struct {
    char items[SET_CAP][SET_ITEM];
    size_t len;
} strset_t;

static bool set_add(strset_t *s, const char *value) {
    for (size_t i = 0; i < s->len; i++) {
        if (strcmp(s->items[i], value) == 0) {
            return false; /* already present */
        }
    }
    if (s->len >= SET_CAP) {
        return false;
    }
    snprintf(s->items[s->len], SET_ITEM, "%s", value);
    s->len++;
    return true;
}

/* ---- check 10: Wayland session ---- */

static int check_wayland(check_result_list_t *r) {
    const char *type = getenv("XDG_SESSION_TYPE");
    if (type != NULL && strcmp(type, "wayland") == 0) {
        return results_add(r, "wayland-session", GROUP, CHECK_OK, "Wayland session detected", "");
    }
    if (type != NULL && *type != '\0') {
        char msg[OSDOCTOR_MSG_CAP];
        snprintf(msg, sizeof msg, "Session type is '%s' (not Wayland)", type);
        return results_add(r, "wayland-session", GROUP, CHECK_WARN, msg, "");
    }
    return results_add(r, "wayland-session", GROUP, CHECK_WARN, "XDG_SESSION_TYPE not set", "");
}

/* ---- check 11: Hyprland socket ---- */

static int check_hyprland_socket(check_result_list_t *r) {
    const char *sig = getenv("HYPRLAND_INSTANCE_SIGNATURE");
    if (sig == NULL || *sig == '\0') {
        return results_add(r, "hyprland-socket", GROUP, CHECK_SKIP,
                           "Hyprland not running (no instance signature)", "");
    }
    const char *xdg = getenv("XDG_RUNTIME_DIR");
    if (xdg == NULL || *xdg == '\0') {
        return results_add(r, "hyprland-socket", GROUP, CHECK_WARN, "XDG_RUNTIME_DIR not set", "");
    }
    char path[4096];
    snprintf(path, sizeof path, "%s/hypr/%s/.socket.sock", xdg, sig);
    if (file_exists(path)) {
        return results_add(r, "hyprland-socket", GROUP, CHECK_OK, "Hyprland socket detected", "");
    }
    return results_add(r, "hyprland-socket", GROUP, CHECK_WARN, "Hyprland socket not found", path);
}

/* ---- check 12: Hyprland bind command validation ---- */

/* Copy the first whitespace-delimited token of `s` into `out`. */
static void first_token(const char *s, char *out, size_t out_size) {
    while (*s == ' ' || *s == '\t') {
        s++;
    }
    size_t i = 0;
    while (*s != '\0' && *s != ' ' && *s != '\t' && i + 1 < out_size) {
        out[i++] = *s++;
    }
    out[i] = '\0';
}

/* Strip one layer of matching surrounding quotes from `s` in place. */
static void strip_quotes(char *s) {
    size_t n = strlen(s);
    if (n >= 2 && (s[0] == '"' || s[0] == '\'') && s[n - 1] == s[0]) {
        memmove(s, s + 1, n - 2);
        s[n - 2] = '\0';
    }
}

/* True if a command token is safe to resolve against PATH (i.e. it is a plain
 * program name or path, not a shell variable or substitution). */
static bool token_is_checkable(const char *tok) {
    if (tok == NULL || *tok == '\0') {
        return false;
    }
    if (tok[0] == '$') {
        return false; /* Hyprland variable, e.g. $terminal */
    }
    if (strpbrk(tok, "$`") != NULL) {
        return false; /* variable / command substitution */
    }
    return true;
}

/* True if the trimmed text before '=' is a Hyprland bind keyword
 * ("bind" optionally followed by flag letters: bind, binde, bindl, ...). */
static bool is_bind_keyword(const char *line, const char *eq) {
    char kw[32];
    size_t len = (size_t)(eq - line);
    char tmp[64];
    if (len >= sizeof tmp) {
        len = sizeof tmp - 1;
    }
    memcpy(tmp, line, len);
    tmp[len] = '\0';
    char *trimmed = str_trim(tmp);
    snprintf(kw, sizeof kw, "%s", trimmed);
    if (!str_starts_with(kw, "bind")) {
        return false;
    }
    for (const char *c = kw; *c != '\0'; c++) {
        if (*c < 'a' || *c > 'z') {
            return false;
        }
    }
    return true;
}

static int check_hyprland_binds(check_result_list_t *r) {
    char *cfg = expand_home_path("~/.config/hypr/hyprland.conf");
    if (cfg == NULL) {
        return -1;
    }
    if (!file_exists(cfg)) {
        free(cfg);
        return results_add(r, "hyprland-binds", GROUP, CHECK_SKIP, "Hyprland config not found",
                           "~/.config/hypr/hyprland.conf");
    }
    char *content = fs_read_file(cfg, CONFIG_MAX_BYTES, NULL);
    free(cfg);
    if (content == NULL) {
        return results_add(r, "hyprland-binds", GROUP, CHECK_SKIP, "Could not read Hyprland config",
                           "");
    }

    strset_t missing = {0};
    int checked = 0;
    size_t nlines = 0;
    char **lines = str_split_lines(content, &nlines);
    for (size_t i = 0; i < nlines; i++) {
        char *line = str_trim(lines[i]);
        if (*line == '#' || !str_starts_with(line, "bind")) {
            continue;
        }
        char *eq = strchr(line, '=');
        if (eq == NULL || !is_bind_keyword(line, eq)) {
            continue;
        }
        /* RHS fields: mods, key, dispatcher, args...  The command lives in
         * `args` only when the dispatcher is exec/execr. */
        char *c1 = strchr(eq + 1, ',');
        if (c1 == NULL) {
            continue;
        }
        char *c2 = strchr(c1 + 1, ',');
        if (c2 == NULL) {
            continue;
        }
        char *c3 = strchr(c2 + 1, ',');

        char dispatcher[64];
        char tmp[128];
        size_t dl = (c3 != NULL) ? (size_t)(c3 - (c2 + 1)) : strlen(c2 + 1);
        if (dl >= sizeof tmp) {
            dl = sizeof tmp - 1;
        }
        memcpy(tmp, c2 + 1, dl);
        tmp[dl] = '\0';
        snprintf(dispatcher, sizeof dispatcher, "%s", str_trim(tmp));
        if (strcmp(dispatcher, "exec") != 0 && strcmp(dispatcher, "execr") != 0) {
            continue;
        }
        if (c3 == NULL) {
            continue; /* exec with no argument */
        }

        char tok[SET_ITEM];
        first_token(c3 + 1, tok, sizeof tok);
        strip_quotes(tok);
        if (!token_is_checkable(tok)) {
            continue;
        }
        checked++;
        char *expanded = expand_home_path(tok);
        bool ok = (expanded != NULL) && is_executable_in_path(expanded);
        free(expanded);
        if (!ok) {
            set_add(&missing, tok);
        }
    }
    str_free_lines(lines, nlines);
    free(content);

    if (missing.len == 0) {
        char msg[OSDOCTOR_MSG_CAP];
        snprintf(msg, sizeof msg, "Hyprland bind commands resolved (%d checked)", checked);
        return results_add(r, "hyprland-binds", GROUP, CHECK_OK, msg, "");
    }
    for (size_t i = 0; i < missing.len; i++) {
        char msg[OSDOCTOR_MSG_CAP];
        snprintf(msg, sizeof msg, "Hyprland bind references missing command: %s", missing.items[i]);
        if (results_add(r, "hyprland-binds", GROUP, CHECK_WARN, msg, "") != 0) {
            return -1;
        }
    }
    return 0;
}

/* ---- check 13: Waybar script validation ---- */

static bool looks_like_local_script(const char *tok) {
    if (tok == NULL || *tok == '\0') {
        return false;
    }
    return str_starts_with(tok, "~/") || str_starts_with(tok, "./") ||
           str_starts_with(tok, "$HOME/") || str_starts_with(tok, "/home/") ||
           str_starts_with(tok, "scripts/") || str_contains(tok, "/scripts/") ||
           str_contains(tok, "/.config/");
}

/* Strip surrounding quotes/backticks and trailing shell punctuation. */
static void strip_script_edges(char *s) {
    size_t n = strlen(s);
    size_t start = 0;
    while (s[start] == '"' || s[start] == '\'' || s[start] == '`') {
        start++;
    }
    while (n > start) {
        char c = s[n - 1];
        if (c == '"' || c == '\'' || c == '`' || c == ';' || c == ',' || c == '&' || c == '|') {
            n--;
        } else {
            break;
        }
    }
    size_t len = n - start;
    memmove(s, s + start, len);
    s[len] = '\0';
}

/* Resolve a referenced script path to an absolute-ish path for existence
 * testing. Relative paths are resolved against the config directory. Returns a
 * malloc'd string the caller frees, or NULL. */
static char *resolve_script(const char *tok, const char *config_dir) {
    if (str_starts_with(tok, "$HOME/")) {
        const char *home = getenv("HOME");
        if (home == NULL) {
            home = "";
        }
        const char *rest = tok + 5; /* keep leading '/' */
        size_t len = strlen(home) + strlen(rest) + 1;
        char *out = malloc(len);
        if (out != NULL) {
            snprintf(out, len, "%s%s", home, rest);
        }
        return out;
    }
    if (tok[0] == '~') {
        return expand_home_path(tok);
    }
    if (tok[0] == '/') {
        return strdup(tok);
    }
    const char *rel = tok;
    if (str_starts_with(rel, "./")) {
        rel += 2;
    }
    size_t len = strlen(config_dir) + 1 + strlen(rel) + 1;
    char *out = malloc(len);
    if (out != NULL) {
        snprintf(out, len, "%s/%s", config_dir, rel);
    }
    return out;
}

static int check_waybar(check_result_list_t *r) {
    char *jsonc = expand_home_path("~/.config/waybar/config.jsonc");
    char *plain = expand_home_path("~/.config/waybar/config");
    char *cfg = NULL;
    if (jsonc != NULL && file_exists(jsonc)) {
        cfg = jsonc;
    } else if (plain != NULL && file_exists(plain)) {
        cfg = plain;
    }
    if (cfg == NULL) {
        free(jsonc);
        free(plain);
        return results_add(r, "waybar-scripts", GROUP, CHECK_SKIP, "Waybar config not found", "");
    }

    /* Config directory for resolving relative script references. */
    char config_dir[4096];
    snprintf(config_dir, sizeof config_dir, "%s", cfg);
    char *slash = strrchr(config_dir, '/');
    if (slash != NULL) {
        *slash = '\0';
    } else {
        snprintf(config_dir, sizeof config_dir, ".");
    }

    char *content = fs_read_file(cfg, CONFIG_MAX_BYTES, NULL);
    free(jsonc);
    free(plain);
    if (content == NULL) {
        return results_add(r, "waybar-scripts", GROUP, CHECK_SKIP, "Could not read Waybar config",
                           "");
    }

    strset_t missing = {0};
    int checked = 0;

    /* Walk every double-quoted JSON string and inspect its whitespace tokens
     * for things that look like local script paths. */
    const char *p = content;
    while (*p != '\0') {
        if (*p != '"') {
            p++;
            continue;
        }
        const char *q = p + 1;
        char buf[2048];
        size_t bi = 0;
        while (*q != '\0' && *q != '"') {
            if (*q == '\\' && q[1] != '\0') {
                if (bi + 1 < sizeof buf) {
                    buf[bi++] = q[1];
                }
                q += 2;
                continue;
            }
            if (bi + 1 < sizeof buf) {
                buf[bi++] = *q;
            }
            q++;
        }
        buf[bi] = '\0';
        p = (*q == '"') ? q + 1 : q;

        char *save = NULL;
        for (char *t = strtok_r(buf, " \t", &save); t != NULL; t = strtok_r(NULL, " \t", &save)) {
            char tok[SET_ITEM];
            snprintf(tok, sizeof tok, "%s", t);
            strip_script_edges(tok);
            if (!looks_like_local_script(tok)) {
                continue;
            }
            checked++;
            char *resolved = resolve_script(tok, config_dir);
            bool exists = (resolved != NULL) && file_exists(resolved);
            free(resolved);
            if (!exists) {
                set_add(&missing, tok);
            }
        }
    }
    free(content);

    if (missing.len == 0) {
        char msg[OSDOCTOR_MSG_CAP];
        if (checked == 0) {
            snprintf(msg, sizeof msg, "Waybar config present (no local scripts referenced)");
        } else {
            snprintf(msg, sizeof msg, "Waybar scripts resolved (%d checked)", checked);
        }
        return results_add(r, "waybar-scripts", GROUP, CHECK_OK, msg, "");
    }
    for (size_t i = 0; i < missing.len; i++) {
        char msg[OSDOCTOR_MSG_CAP];
        snprintf(msg, sizeof msg, "Waybar config references missing script: %s", missing.items[i]);
        if (results_add(r, "waybar-scripts", GROUP, CHECK_FAIL, msg, "") != 0) {
            return -1;
        }
    }
    return 0;
}

int run_desktop_checks(check_result_list_t *r) {
    if (check_wayland(r) != 0) {
        return -1;
    }
    if (check_hyprland_socket(r) != 0) {
        return -1;
    }
    if (check_hyprland_binds(r) != 0) {
        return -1;
    }
    if (check_waybar(r) != 0) {
        return -1;
    }
    return 0;
}
