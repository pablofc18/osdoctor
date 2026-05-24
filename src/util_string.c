#include "util_string.h"

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

char *str_trim(char *s) {
    if (s == NULL) {
        return s;
    }
    char *start = s;
    while (*start != '\0' && isspace((unsigned char)*start)) {
        start++;
    }
    char *end = start + strlen(start);
    while (end > start && isspace((unsigned char)end[-1])) {
        end--;
    }
    *end = '\0';
    return start;
}

bool str_starts_with(const char *s, const char *prefix) {
    if (s == NULL || prefix == NULL) {
        return false;
    }
    return strncmp(s, prefix, strlen(prefix)) == 0;
}

bool str_contains(const char *s, const char *needle) {
    if (s == NULL || needle == NULL) {
        return false;
    }
    return strstr(s, needle) != NULL;
}

char **str_split_lines(const char *input, size_t *out_count) {
    if (out_count != NULL) {
        *out_count = 0;
    }
    if (input == NULL || *input == '\0') {
        return NULL;
    }

    /* First pass: count lines. */
    size_t count = 0;
    for (const char *p = input; *p != '\0';) {
        const char *nl = strchr(p, '\n');
        count++;
        if (nl == NULL) {
            break;
        }
        p = nl + 1;
        if (*p == '\0') { /* trailing newline: no empty final element */
            break;
        }
    }

    char **lines = malloc(count * sizeof *lines);
    if (lines == NULL) {
        return NULL;
    }

    /* Second pass: copy lines. */
    size_t idx = 0;
    const char *p = input;
    while (*p != '\0' && idx < count) {
        const char *nl = strchr(p, '\n');
        size_t len = (nl != NULL) ? (size_t)(nl - p) : strlen(p);
        if (len > 0 && p[len - 1] == '\r') { /* strip CR from CRLF */
            len--;
        }
        char *line = malloc(len + 1);
        if (line == NULL) {
            for (size_t k = 0; k < idx; k++) {
                free(lines[k]);
            }
            free(lines);
            return NULL;
        }
        memcpy(line, p, len);
        line[len] = '\0';
        lines[idx++] = line;
        if (nl == NULL) {
            break;
        }
        p = nl + 1;
        if (*p == '\0') {
            break;
        }
    }

    if (out_count != NULL) {
        *out_count = idx;
    }
    return lines;
}

void str_free_lines(char **lines, size_t count) {
    if (lines == NULL) {
        return;
    }
    for (size_t i = 0; i < count; i++) {
        free(lines[i]);
    }
    free(lines);
}

int json_escape(const char *in, char *out, size_t out_size) {
    if (out == NULL || out_size == 0) {
        return -1;
    }
    out[0] = '\0';
    if (in == NULL) {
        return 0;
    }

    size_t o = 0;
    for (const unsigned char *p = (const unsigned char *)in; *p != '\0'; p++) {
        char esc[8];
        size_t need;
        switch (*p) {
        case '"':
            esc[0] = '\\';
            esc[1] = '"';
            need = 2;
            break;
        case '\\':
            esc[0] = '\\';
            esc[1] = '\\';
            need = 2;
            break;
        case '\n':
            esc[0] = '\\';
            esc[1] = 'n';
            need = 2;
            break;
        case '\r':
            esc[0] = '\\';
            esc[1] = 'r';
            need = 2;
            break;
        case '\t':
            esc[0] = '\\';
            esc[1] = 't';
            need = 2;
            break;
        case '\b':
            esc[0] = '\\';
            esc[1] = 'b';
            need = 2;
            break;
        case '\f':
            esc[0] = '\\';
            esc[1] = 'f';
            need = 2;
            break;
        default:
            if (*p < 0x20) {
                snprintf(esc, sizeof esc, "\\u%04x", (unsigned)*p);
                need = 6;
            } else {
                esc[0] = (char)*p;
                need = 1;
            }
            break;
        }
        if (o + need > out_size - 1) { /* would not fit + NUL: stop cleanly */
            out[o] = '\0';
            return -1;
        }
        memcpy(out + o, esc, need);
        o += need;
    }
    out[o] = '\0';
    return 0;
}
