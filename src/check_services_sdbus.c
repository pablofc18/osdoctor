/*
 * check_services_sdbus.c - sd-bus (libsystemd) backend for the Services checks.
 *
 * Compiled only when built with USE_LIBSYSTEMD=1. Queries the systemd manager
 * directly over D-Bus instead of shelling out to systemctl:
 *
 *   org.freedesktop.systemd1 / /org/freedesktop/systemd1
 *   org.freedesktop.systemd1.Manager.ListUnitsFiltered(in as states)
 *
 * with states = ["failed"], which returns an array of unit structs whose first
 * field is the unit name. This avoids the subprocess, the text parsing and the
 * stderr handling of the systemctl backend.
 */
#include "check_services_internal.h"

#include <stdint.h>
#include <systemd/sd-bus.h>

svc_query_status_t svc_query_sdbus(svc_scope_t scope, int *count, char *detail,
                                   size_t detail_size) {
    sd_bus *bus = NULL;
    sd_bus_error err = SD_BUS_ERROR_NULL;
    sd_bus_message *reply = NULL;
    svc_query_status_t result = SVC_QUERY_UNAVAILABLE;
    size_t used = 0;
    int n = 0;
    int r;

    *count = 0;
    if (detail_size > 0) {
        detail[0] = '\0';
    }

    r = (scope == SVC_SCOPE_SYSTEM) ? sd_bus_open_system(&bus) : sd_bus_open_user(&bus);
    if (r < 0) {
        /* A missing user bus is the normal "no user session" case (SKIP); a
         * missing system bus is a hard error that triggers the systemctl
         * fallback in the caller. */
        result = (scope == SVC_SCOPE_USER) ? SVC_QUERY_NO_SESSION : SVC_QUERY_UNAVAILABLE;
        goto finish;
    }

    r = sd_bus_call_method(bus, "org.freedesktop.systemd1", "/org/freedesktop/systemd1",
                           "org.freedesktop.systemd1.Manager", "ListUnitsFiltered", &err, &reply,
                           "as", 1, "failed");
    if (r < 0) {
        goto finish;
    }

    r = sd_bus_message_enter_container(reply, SD_BUS_TYPE_ARRAY, "(ssssssouso)");
    if (r < 0) {
        goto finish;
    }

    {
        const char *name, *description, *load_state, *active_state, *sub_state, *following,
            *unit_path, *job_type, *job_path;
        uint32_t job_id;
        while ((r = sd_bus_message_read(reply, "(ssssssouso)", &name, &description, &load_state,
                                        &active_state, &sub_state, &following, &unit_path, &job_id,
                                        &job_type, &job_path)) > 0) {
            n++;
            svc_detail_append(detail, detail_size, &used, name);
        }
    }
    if (r < 0) { /* malformed reply */
        goto finish;
    }

    sd_bus_message_exit_container(reply);
    *count = n;
    result = SVC_QUERY_OK;

finish:
    sd_bus_error_free(&err);
    sd_bus_message_unref(reply);
    sd_bus_flush_close_unref(bus);
    return result;
}
