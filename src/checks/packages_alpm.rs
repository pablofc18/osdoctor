//! libalpm backend for the orphan-package check. Compiled only with the
//! `libalpm` cargo feature.
//! Reads the local pacman database directly instead of shelling out to
//! `pacman -Qdtq`. A package is an orphan when it was installed as a dependency
//! and nothing (neither a hard dependency nor an optional one) still requires
//! it - the same set `pacman -Qdt` reports.

use super::packages::{Query, QueryStatus, DETAIL_CAP};
use crate::util::strings::Detail;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_void};

#[repr(C)]
struct AlpmHandle {
    _private: [u8; 0],
}

#[repr(C)]
struct AlpmDb {
    _private: [u8; 0],
}

#[repr(C)]
struct AlpmPkg {
    _private: [u8; 0],
}

/// Mirrors `alpm_list_t` from alpm_list.h.
#[repr(C)]
struct AlpmList {
    data: *mut c_void,
    prev: *mut AlpmList,
    next: *mut AlpmList,
}

/// `ALPM_PKG_REASON_DEPEND` from alpm.h.
const ALPM_PKG_REASON_DEPEND: c_int = 1;

#[link(name = "alpm")]
extern "C" {
    fn alpm_initialize(
        root: *const c_char,
        dbpath: *const c_char,
        err: *mut c_int,
    ) -> *mut AlpmHandle;
    fn alpm_release(handle: *mut AlpmHandle) -> c_int;
    fn alpm_get_localdb(handle: *mut AlpmHandle) -> *mut AlpmDb;
    fn alpm_db_get_pkgcache(db: *mut AlpmDb) -> *mut AlpmList;
    fn alpm_list_next(list: *const AlpmList) -> *mut AlpmList;
    fn alpm_list_free(list: *mut AlpmList);
    fn alpm_list_free_inner(list: *mut AlpmList, f: Option<unsafe extern "C" fn(*mut c_void)>);
    fn alpm_pkg_get_reason(pkg: *mut AlpmPkg) -> c_int;
    fn alpm_pkg_get_name(pkg: *mut AlpmPkg) -> *const c_char;
    fn alpm_pkg_compute_requiredby(pkg: *mut AlpmPkg) -> *mut AlpmList;
    fn alpm_pkg_compute_optionalfor(pkg: *mut AlpmPkg) -> *mut AlpmList;
}

/// Releases the libalpm handle on drop, so every early return cleans up.
struct Handle(*mut AlpmHandle);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle is non-null (checked at construction) and
        // released exactly once.
        unsafe {
            alpm_release(self.0);
        }
    }
}

/// Free a list of malloc'd strings, as alpm.h's FREELIST macro does.
///
/// SAFETY: `list` must be null or a list returned by libalpm whose node data
/// was allocated with malloc and is owned by the caller.
unsafe fn free_list(list: *mut AlpmList) {
    alpm_list_free_inner(list, Some(libc::free));
    alpm_list_free(list);
}

fn unavailable() -> Query {
    Query {
        status: QueryStatus::Unavailable,
        count: 0,
        detail: String::new(),
    }
}

/// Enumerate orphan packages by reading the local pacman DB via libalpm.
/// Returns Unavailable on any hard error (init/db failure) so the caller falls
/// back to the pacman shell-out.
pub fn query() -> Query {
    // SAFETY: plain FFI calls into libalpm following its API contract. The
    // package cache is owned by the handle (not freed here); the lists from
    // compute_requiredby/optionalfor are owned by us and freed with
    // free_list. Package names borrow from the cache, which outlives the loop.
    unsafe {
        let mut err: c_int = 0;
        let raw = alpm_initialize(c"/".as_ptr(), c"/var/lib/pacman/".as_ptr(), &mut err);
        if raw.is_null() {
            return unavailable();
        }
        let handle = Handle(raw);

        let db = alpm_get_localdb(handle.0);
        if db.is_null() {
            return unavailable();
        }

        // An empty local DB (null cache) is unusual but valid: zero orphans.
        let mut detail = Detail::new(DETAIL_CAP);
        let mut count = 0;
        let mut node = alpm_db_get_pkgcache(db);
        while !node.is_null() {
            let pkg = (*node).data as *mut AlpmPkg;
            node = alpm_list_next(node);
            if alpm_pkg_get_reason(pkg) != ALPM_PKG_REASON_DEPEND {
                continue;
            }
            let requiredby = alpm_pkg_compute_requiredby(pkg);
            let optionalfor = alpm_pkg_compute_optionalfor(pkg);
            let is_orphan = requiredby.is_null() && optionalfor.is_null();
            free_list(requiredby);
            free_list(optionalfor);
            if !is_orphan {
                continue;
            }
            count += 1;
            let name = alpm_pkg_get_name(pkg);
            if !name.is_null() {
                detail.append(&CStr::from_ptr(name).to_string_lossy());
            }
        }
        drop(handle);

        Query {
            status: QueryStatus::Ok,
            count,
            detail: detail.into_string(),
        }
    }
}
