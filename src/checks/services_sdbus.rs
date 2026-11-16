//! sd-bus (libsystemd) backend for the Services checks (replaces
//! check_services_sdbus.c). Compiled only with the `libsystemd` cargo feature.
//! Queries the systemd manager directly over D-Bus instead of shelling out to
//! systemctl:
//!
//!   org.freedesktop.systemd1 / /org/freedesktop/systemd1
//!   org.freedesktop.systemd1.Manager.ListUnitsFiltered(in as states)
//!
//! with states = ["failed"], which returns an array of unit structs whose first
//! field is the unit name.

use super::services::{Query, QueryStatus, Scope, DETAIL_CAP};
use crate::util::strings::Detail;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_uint};
use std::ptr;

#[repr(C)]
struct SdBus {
    _private: [u8; 0],
}

#[repr(C)]
struct SdBusMessage {
    _private: [u8; 0],
}

/// Mirrors `sd_bus_error` from sd-bus-protocol.h.
#[repr(C)]
struct SdBusError {
    name: *const c_char,
    message: *const c_char,
    need_free: c_int,
}

const SD_BUS_TYPE_ARRAY: c_char = b'a' as c_char;
const UNIT_STRUCT: &[u8] = b"(ssssssouso)\0";

#[link(name = "systemd")]
extern "C" {
    fn sd_bus_open_system(ret: *mut *mut SdBus) -> c_int;
    fn sd_bus_open_user(ret: *mut *mut SdBus) -> c_int;
    fn sd_bus_flush_close_unref(bus: *mut SdBus) -> *mut SdBus;
    fn sd_bus_call_method(
        bus: *mut SdBus,
        destination: *const c_char,
        path: *const c_char,
        interface: *const c_char,
        member: *const c_char,
        ret_error: *mut SdBusError,
        ret_reply: *mut *mut SdBusMessage,
        types: *const c_char,
        ...
    ) -> c_int;
    fn sd_bus_message_enter_container(
        m: *mut SdBusMessage,
        type_: c_char,
        contents: *const c_char,
    ) -> c_int;
    fn sd_bus_message_read(m: *mut SdBusMessage, types: *const c_char, ...) -> c_int;
    fn sd_bus_message_exit_container(m: *mut SdBusMessage) -> c_int;
    fn sd_bus_message_unref(m: *mut SdBusMessage) -> *mut SdBusMessage;
    fn sd_bus_error_free(e: *mut SdBusError);
}

fn cstr(bytes: &'static [u8]) -> *const c_char {
    debug_assert_eq!(bytes.last(), Some(&0));
    bytes.as_ptr().cast()
}

/// Owns the bus connection, reply and error, releasing them on drop (the
/// `finish:` label of the C backend).
struct Session {
    bus: *mut SdBus,
    reply: *mut SdBusMessage,
    err: SdBusError,
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: each pointer is either null (all three functions accept
        // NULL) or was returned by sd-bus and is released exactly once here.
        unsafe {
            sd_bus_error_free(&mut self.err);
            sd_bus_message_unref(self.reply);
            sd_bus_flush_close_unref(self.bus);
        }
    }
}

fn status_only(status: QueryStatus) -> Query {
    Query {
        status,
        count: 0,
        detail: String::new(),
    }
}

/// Query failed units for `scope` over sd-bus. A missing user bus is the
/// normal "no user session" case (NoSession -> Skip); a missing system bus or
/// a method-call/reply failure is a hard error (Unavailable), which makes the
/// caller fall back to systemctl.
pub fn query(scope: Scope) -> Query {
    let mut s = Session {
        bus: ptr::null_mut(),
        reply: ptr::null_mut(),
        err: SdBusError {
            name: ptr::null(),
            message: ptr::null(),
            need_free: 0,
        },
    };

    // SAFETY: plain FFI calls into libsystemd following the sd-bus API
    // contract; every out-pointer refers to a live field of `s`, all strings
    // are NUL-terminated literals, and variadic arguments match `types`.
    unsafe {
        let r = match scope {
            Scope::System => sd_bus_open_system(&mut s.bus),
            Scope::User => sd_bus_open_user(&mut s.bus),
        };
        if r < 0 {
            return status_only(match scope {
                Scope::User => QueryStatus::NoSession,
                Scope::System => QueryStatus::Unavailable,
            });
        }

        let r = sd_bus_call_method(
            s.bus,
            cstr(b"org.freedesktop.systemd1\0"),
            cstr(b"/org/freedesktop/systemd1\0"),
            cstr(b"org.freedesktop.systemd1.Manager\0"),
            cstr(b"ListUnitsFiltered\0"),
            &mut s.err,
            &mut s.reply,
            cstr(b"as\0"),
            1 as c_uint,
            cstr(b"failed\0"),
        );
        if r < 0 {
            return status_only(QueryStatus::Unavailable);
        }

        if sd_bus_message_enter_container(s.reply, SD_BUS_TYPE_ARRAY, cstr(UNIT_STRUCT)) < 0 {
            return status_only(QueryStatus::Unavailable);
        }

        let mut detail = Detail::new(DETAIL_CAP);
        let mut count = 0;
        let r = loop {
            let mut name: *const c_char = ptr::null();
            let mut description: *const c_char = ptr::null();
            let mut load_state: *const c_char = ptr::null();
            let mut active_state: *const c_char = ptr::null();
            let mut sub_state: *const c_char = ptr::null();
            let mut following: *const c_char = ptr::null();
            let mut unit_path: *const c_char = ptr::null();
            let mut job_id: u32 = 0;
            let mut job_type: *const c_char = ptr::null();
            let mut job_path: *const c_char = ptr::null();
            let r = sd_bus_message_read(
                s.reply,
                cstr(UNIT_STRUCT),
                &mut name,
                &mut description,
                &mut load_state,
                &mut active_state,
                &mut sub_state,
                &mut following,
                &mut unit_path,
                &mut job_id,
                &mut job_type,
                &mut job_path,
            );
            if r <= 0 {
                break r;
            }
            count += 1;
            if !name.is_null() {
                // `name` borrows from the reply, which outlives this loop.
                detail.append(&CStr::from_ptr(name).to_string_lossy());
            }
        };
        if r < 0 {
            return status_only(QueryStatus::Unavailable); // malformed reply
        }
        sd_bus_message_exit_container(s.reply);

        Query {
            status: QueryStatus::Ok,
            count,
            detail: detail.into_string(),
        }
    }
}
