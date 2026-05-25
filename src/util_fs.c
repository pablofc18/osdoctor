#include "util_fs.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

bool file_exists(const char *path) {
    return path != NULL && access(path, F_OK) == 0;
}

bool file_readable(const char *path) {
    return path != NULL && access(path, R_OK) == 0;
}

bool path_writable(const char *path) {
    return path != NULL && access(path, W_OK) == 0;
}

bool is_executable_in_path(const char *cmd) {
    if (cmd == NULL || *cmd == '\0') {
        return false;
    }
    if (strchr(cmd, '/') != NULL) {
        return access(cmd, X_OK) == 0;
    }

    const char *path = getenv("PATH");
    if (path == NULL || *path == '\0') {
        path = "/usr/local/bin:/usr/bin:/bin";
    }
    char *dup = strdup(path);
    if (dup == NULL) {
        return false;
    }

    bool found = false;
    char *save = NULL;
    for (char *dir = strtok_r(dup, ":", &save); dir != NULL; dir = strtok_r(NULL, ":", &save)) {
        const char *d = (*dir != '\0') ? dir : "."; /* empty PATH entry == CWD */
        char full[4096];
        if (join_path(d, cmd, full, sizeof full) == 0 && access(full, X_OK) == 0) {
            found = true;
            break;
        }
    }
    free(dup);
    return found;
}

char *expand_home_path(const char *path) {
    if (path == NULL) {
        return NULL;
    }
    if (path[0] == '~' && (path[1] == '/' || path[1] == '\0')) {
        const char *home = getenv("HOME");
        if (home == NULL) {
            home = "";
        }
        const char *rest = path + 1; /* keeps leading '/' or is "" */
        size_t len = strlen(home) + strlen(rest) + 1;
        char *out = malloc(len);
        if (out == NULL) {
            return NULL;
        }
        snprintf(out, len, "%s%s", home, rest);
        return out;
    }
    return strdup(path);
}

int join_path(const char *a, const char *b, char *out, size_t out_size) {
    if (a == NULL || b == NULL || out == NULL || out_size == 0) {
        return -1;
    }
    size_t la = strlen(a);
    bool a_has_slash = (la > 0 && a[la - 1] == '/');
    while (*b == '/') { /* avoid doubled separators */
        b++;
    }
    int n = a_has_slash ? snprintf(out, out_size, "%s%s", a, b)
                        : snprintf(out, out_size, "%s/%s", a, b);
    if (n < 0 || (size_t)n >= out_size) {
        return -1;
    }
    return 0;
}

char *fs_read_file(const char *path, size_t max_bytes, size_t *out_len) {
    if (out_len != NULL) {
        *out_len = 0;
    }
    FILE *fp = fopen(path, "rb");
    if (fp == NULL) {
        return NULL;
    }
    char *buf = malloc(max_bytes + 1);
    if (buf == NULL) {
        fclose(fp);
        return NULL;
    }
    size_t n = fread(buf, 1, max_bytes, fp);
    buf[n] = '\0';
    fclose(fp);
    if (out_len != NULL) {
        *out_len = n;
    }
    return buf;
}
