/*
 * check_system.c - kernel, disk, /tmp, and memory checks.
 *
 * These all use direct syscalls / /proc and never shell out.
 */
#include "checks.h"
#include "osdoctor.h"
#include "util_fs.h"

#include <stdio.h>
#include <sys/statvfs.h>
#include <sys/utsname.h>

static const char *GROUP = "System";

#define GIB (1024ULL * 1024ULL * 1024ULL)

int run_system_checks(check_result_list_t *r) {
    char msg[OSDOCTOR_MSG_CAP];
    char detail[OSDOCTOR_DETAIL_CAP];

    /* 1. Kernel version */
    struct utsname uts;
    if (uname(&uts) == 0) {
        snprintf(msg, sizeof msg, "Kernel detected: %s", uts.release);
        snprintf(detail, sizeof detail, "%s %s %s", uts.sysname, uts.release, uts.version);
        if (results_add(r, "kernel-version", GROUP, CHECK_OK, msg, detail) != 0) {
            return -1;
        }
    } else if (results_add(r, "kernel-version", GROUP, CHECK_WARN, "Could not read kernel version",
                           "") != 0) {
        return -1;
    }

    /* 2. Root filesystem free space */
    struct statvfs vfs;
    if (statvfs("/", &vfs) == 0) {
        unsigned long long free_bytes =
            (unsigned long long)vfs.f_bavail * (unsigned long long)vfs.f_frsize;
        unsigned long long free_gib = free_bytes / GIB;
        check_status_t status = CHECK_OK;
        if (free_bytes < 2ULL * GIB) {
            status = CHECK_FAIL;
        } else if (free_bytes < 10ULL * GIB) {
            status = CHECK_WARN;
        }
        snprintf(msg, sizeof msg, "Root filesystem has %llu GB free", free_gib);
        if (results_add(r, "root-disk-space", GROUP, status, msg, "") != 0) {
            return -1;
        }
    } else if (results_add(r, "root-disk-space", GROUP, CHECK_WARN,
                           "Could not stat root filesystem", "") != 0) {
        return -1;
    }

    /* 3. /tmp writable */
    if (path_writable("/tmp")) {
        if (results_add(r, "tmp-writable", GROUP, CHECK_OK, "/tmp is writable", "") != 0) {
            return -1;
        }
    } else if (results_add(r, "tmp-writable", GROUP, CHECK_FAIL, "/tmp is not writable", "") != 0) {
        return -1;
    }

    /* 4. Memory info from /proc/meminfo */
    FILE *fp = fopen("/proc/meminfo", "r");
    if (fp == NULL) {
        if (results_add(r, "memory-info", GROUP, CHECK_WARN,
                        "Memory info unavailable (/proc/meminfo)", "") != 0) {
            return -1;
        }
    } else {
        unsigned long long total_kb = 0;
        unsigned long long avail_kb = 0;
        int got = 0; /* bit 0: total, bit 1: available */
        char line[256];
        while (fgets(line, sizeof line, fp) != NULL) {
            if (sscanf(line, "MemTotal: %llu kB", &total_kb) == 1) {
                got |= 1;
            } else if (sscanf(line, "MemAvailable: %llu kB", &avail_kb) == 1) {
                got |= 2;
            }
            if (got == 3) {
                break;
            }
        }
        fclose(fp);

        if ((got & 1) == 0) {
            if (results_add(r, "memory-info", GROUP, CHECK_WARN, "Could not parse /proc/meminfo",
                            "") != 0) {
                return -1;
            }
        } else {
            unsigned long long total_mb = total_kb / 1024;
            unsigned long long avail_mb = avail_kb / 1024;
            /* WARN when less than 10% of RAM is available (memory pressure). */
            check_status_t status = CHECK_OK;
            if ((got & 2) != 0 && total_kb > 0 && avail_kb * 10ULL < total_kb) {
                status = CHECK_WARN;
                snprintf(msg, sizeof msg, "Low available memory: %llu MB of %llu MB", avail_mb,
                         total_mb);
            } else {
                snprintf(msg, sizeof msg, "Memory: %llu MB available of %llu MB", avail_mb,
                         total_mb);
            }
            if (results_add(r, "memory-info", GROUP, status, msg, "") != 0) {
                return -1;
            }
        }
    }

    return 0;
}
