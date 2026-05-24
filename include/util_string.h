/*
 * util_string.h - small string utilities.
 */
#ifndef OSDOCTOR_UTIL_STRING_H
#define OSDOCTOR_UTIL_STRING_H

#include <stdbool.h>
#include <stddef.h>

/* Trim leading/trailing ASCII whitespace in place. Returns a pointer into
 * `s` at the first non-space character (the trailing run is NUL-terminated). */
char *str_trim(char *s);

bool str_starts_with(const char *s, const char *prefix);
bool str_contains(const char *s, const char *needle);

/*
 * Split `input` into lines on '\n' (a trailing '\r' is stripped). A trailing
 * newline does not produce an empty final element. Returns a malloc'd array of
 * malloc'd strings and sets *out_count. Returns NULL (count 0) for empty/NULL
 * input or on allocation failure. Free with str_free_lines().
 */
char **str_split_lines(const char *input, size_t *out_count);
void str_free_lines(char **lines, size_t count);

/*
 * Escape `in` for embedding inside a JSON string (no surrounding quotes) into
 * `out`. Always NUL-terminates. Returns 0 on success, -1 if the output was
 * truncated (no partial escape sequence is ever emitted).
 */
int json_escape(const char *in, char *out, size_t out_size);

#endif /* OSDOCTOR_UTIL_STRING_H */
