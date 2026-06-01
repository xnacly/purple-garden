use std::ffi::c_void;
use std::ptr::NonNull;

/// Native file-offset type used by `mmap(2)` on the supported 64-bit targets.
#[cfg(target_pointer_width = "64")]
type OffT = i64;

unsafe extern "C" {
    /// Maps files or anonymous pages into this process using the platform C ABI.
    #[link_name = "mmap"]
    fn sys_mmap(
        addr: *mut c_void,
        len: usize,
        prot: i32,
        flags: i32,
        fd: i32,
        offset: OffT,
    ) -> *mut c_void;
    /// Changes page protections for an existing mapping using the platform C ABI.
    #[link_name = "mprotect"]
    fn sys_mprotect(addr: *mut c_void, len: usize, prot: i32) -> i32;
    /// Releases a mapping created by `mmap(2)` using the platform C ABI.
    #[link_name = "munmap"]
    fn sys_munmap(addr: *mut c_void, len: usize) -> i32;
}

#[cfg(target_os = "linux")]
const MAP_PRIVATE: i32 = 0x0002;
#[cfg(target_os = "linux")]
const MAP_ANONYMOUS: i32 = 0x0020;

#[cfg(target_os = "macos")]
const MAP_PRIVATE: i32 = 0x0002;
#[cfg(target_os = "macos")]
const MAP_ANONYMOUS: i32 = 0x1000;

const MAP_FAILED: *mut c_void = !0usize as *mut c_void;

/// Page permissions passed to [`mmap`] and [`mprotect`].
///
/// Not an enum: `READ`, `WRITE`, and `EXEC` can be combined for mappings that
/// need multiple permissions.
#[derive(Clone, Copy)]
pub struct MmapProt(i32);

impl MmapProt {
    /// No permissions.
    pub const NONE: Self = Self(0x00);
    /// Pages can be read.
    pub const READ: Self = Self(0x01);
    /// Pages can be written.
    pub const WRITE: Self = Self(0x02);
    /// Pages can be executed.
    pub const EXEC: Self = Self(0x04);

    /// Returns the raw platform permission bits.
    #[must_use]
    pub fn bits(self) -> i32 {
        self.0
    }
}

impl std::ops::BitOr for MmapProt {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

/// Mapping flags passed to [`mmap`].
#[derive(Clone, Copy)]
pub struct MmapFlags(i32);

impl MmapFlags {
    /// Changes are private.
    pub const PRIVATE: Self = Self(MAP_PRIVATE);
    /// Allocate zero-filled memory not backed by a file.
    pub const ANONYMOUS: Self = Self(MAP_ANONYMOUS);

    /// Returns the raw platform flag bits.
    #[must_use]
    pub fn bits(self) -> i32 {
        self.0
    }
}

impl std::ops::BitOr for MmapFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

#[inline(always)]
/// Creates a memory mapping and returns its non-null base address.
///
/// `ptr` is an optional address hint, `length` is the mapping size in bytes,
/// `prot` controls page permissions, `flags` controls mapping behavior, and
/// `fd`/`offset` select the mapped file region. For anonymous mappings, pass
/// `MmapFlags::ANONYMOUS`, `fd = -1`, and `offset = 0`.
pub fn mmap(
    ptr: Option<NonNull<u8>>,
    length: usize,
    prot: MmapProt,
    flags: MmapFlags,
    fd: i32,
    offset: i64,
) -> Result<NonNull<u8>, String> {
    let ret = unsafe {
        sys_mmap(
            ptr.map_or(std::ptr::null_mut(), |ptr| ptr.as_ptr().cast()),
            length,
            prot.bits(),
            flags.bits(),
            fd,
            offset as OffT,
        )
    };

    if ret == MAP_FAILED {
        return Err(os_error("mmap"));
    }

    Ok(unsafe { NonNull::new_unchecked(ret.cast()) })
}

#[inline(always)]
/// Changes page permissions for an existing mapping.
pub fn mprotect(ptr: NonNull<u8>, length: usize, prot: MmapProt) -> Result<(), String> {
    if unsafe { sys_mprotect(ptr.as_ptr().cast(), length, prot.bits()) } == -1 {
        return Err(os_error("mprotect"));
    }

    Ok(())
}

#[inline(always)]
/// Releases a memory mapping created by [`mmap`].
pub fn munmap(ptr: NonNull<u8>, size: usize) -> Result<(), String> {
    if unsafe { sys_munmap(ptr.as_ptr().cast(), size) } == -1 {
        return Err(os_error("munmap"));
    }

    Ok(())
}

fn os_error(op: &str) -> String {
    let err = std::io::Error::last_os_error();
    format!(
        "{op} failed (errno {}): {err}",
        err.raw_os_error().unwrap_or(0)
    )
}

#[cfg(test)]
mod tests {
    use super::{MmapFlags, MmapProt, mmap, mprotect, munmap};

    #[test]
    fn anonymous_mapping_is_zeroed_and_writable() {
        let len = 4096;
        let ptr = mmap(
            None,
            len,
            MmapProt::READ | MmapProt::WRITE,
            MmapFlags::PRIVATE | MmapFlags::ANONYMOUS,
            -1,
            0,
        )
        .expect("anonymous mmap");

        let bytes = unsafe { std::slice::from_raw_parts_mut(ptr.as_ptr(), len) };
        assert!(bytes.iter().all(|byte| *byte == 0));

        bytes[0] = 0xab;
        bytes[len - 1] = 0xcd;
        assert_eq!(bytes[0], 0xab);
        assert_eq!(bytes[len - 1], 0xcd);

        munmap(ptr, len).expect("munmap");
    }

    #[test]
    fn mapping_permissions_can_be_changed() {
        let len = 4096;
        let ptr = mmap(
            None,
            len,
            MmapProt::READ | MmapProt::WRITE,
            MmapFlags::PRIVATE | MmapFlags::ANONYMOUS,
            -1,
            0,
        )
        .expect("anonymous mmap");

        mprotect(ptr, len, MmapProt::READ).expect("mprotect read-only");
        mprotect(ptr, len, MmapProt::READ | MmapProt::WRITE).expect("mprotect read-write");

        unsafe { ptr.as_ptr().write(0x7f) };
        assert_eq!(unsafe { ptr.as_ptr().read() }, 0x7f);

        munmap(ptr, len).expect("munmap");
    }
}
