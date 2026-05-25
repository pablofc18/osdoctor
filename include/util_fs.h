/*
 * util_fs.h - filesystem helpers.
 */
#ifndef OSDOCTOR_UTIL_FS_H
#define OSDOCTOR_UTIL_FS_H

#include <stdbool.h>
#include <stddef.h>

bool file_exists(const char *path);   /* path exists (any type)        */
bool file_readable(const char *path); /* readable by current process   */
bool path_writable(const char *path); /* writable by current process   */

/*
 * True if `cmd` is executable. If `cmd` contains '/', it is checked directly
 * with access(X_OK). Otherwise each PATH entry is searched.
 */
bool is_executable_in_path(const char *cmd);

/*
 * Expand a leading "~" or "~/" to $HOME. Returns a newly malloc'd string the
 * caller must free, or NULL on allocation failure / NULL input.
 */
char *expand_home_path(const char *path);

/*
 * Join `a` and `b` with exactly one '/' into `out`. Returns 0 on success or
 * -1 if the result was truncated / arguments were invalid.
 */
int join_path(const char *a, const char *b, char *out, size_t out_size);

/*
 * Read up to `max_bytes` of a file into a newly malloc'd, NUL-terminated
 * buffer. Sets *out_len (if non-NULL) to the number of bytes read. Returns
 * NULL on error. Caller frees the buffer.
 */
char *fs_read_file(const char *path, size_t max_bytes, size_t *out_len);

#endif /* OSDOCTOR_UTIL_FS_H */
