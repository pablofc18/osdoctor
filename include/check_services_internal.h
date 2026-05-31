/*
 * check_services_internal.h - internals of the Services checks exposed for
 * unit testing and shared between the orchestrator and the optional sd-bus
 * backend. Not part of the public CLI surface.
 */
#ifndef OSDOCTOR_CHECK_SERVICES_INTERNAL_H
#define OSDOCTOR_CHECK_SERVICES_INTERNAL_H

#include "osdoctor.h"

#include <stddef.h>

/* Which systemd manager a query targets. */
typedef enum { SVC_SCOPE_SYSTEM, SVC_SCOPE_USER } svc_scope_t;

/* Outcome of asking a backend for the failed units in one scope. */
typedef enum {
    SVC_QUERY_OK = 0,      /* authoritative answer: *count and detail filled  */
    SVC_QUERY_NO_SESSION,  /* no manager/bus for this scope    -> SKIP        */
    SVC_QUERY_UNAVAILABLE, /* backend can't run / hard error   -> try fallback */
} svc_query_status_t;

/*
 * Append a unit name to a bounded, comma-separated sample in `detail`.
 * `*used` tracks the current length and is updated; nothing is written once
 * the buffer is full, so `detail` stays NUL-terminated. No-op on NULL inputs.
 * The caller must initialise `detail[0] = '\0'` and `*used = 0`.
 */
void svc_detail_append(char *detail, size_t detail_size, size_t *used, const char *unit);

/*
 * Map a backend query result for one scope onto a check result and append it
 * to `r` (id, group, status and message chosen to match the documented
 * output). Returns 0 on success or -1 on allocation failure.
 */
int svc_emit(check_result_list_t *r, svc_scope_t scope, svc_query_status_t status, int count,
             const char *detail);

#ifdef OSDOCTOR_HAVE_LIBSYSTEMD
/*
 * Query failed units for `scope` over sd-bus. Fills *count (all failed units)
 * and `detail` (a bounded comma-separated sample of names). See the enum for
 * the status contract; a missing user bus yields SVC_QUERY_NO_SESSION, while a
 * system-bus or method-call failure yields SVC_QUERY_UNAVAILABLE.
 */
svc_query_status_t svc_query_sdbus(svc_scope_t scope, int *count, char *detail, size_t detail_size);
#endif

#endif /* OSDOCTOR_CHECK_SERVICES_INTERNAL_H */
