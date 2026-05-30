use std::ffi::c_void;
use std::ptr::NonNull;

#[cfg(target_pointer_width = "64")]
type OffT = i64;

unsafe extern "C" {
    #[link_name = "mmap"]
    fn sys_mmap(
        addr: *mut c_void,
        len: usize,
        prot: i32,
        flags: i32,
        fd: i32,
        offset: OffT,
    ) -> *mut c_void;
    #[link_name = "mprotect"]
    fn sys_mprotect(addr: *mut c_void, len: usize, prot: i32) -> i32;
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

// Not an enum, since NONE, READ, WRITE and EXEC are not mutually exclusive.
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

#[derive(Clone, Copy)]
pub struct MmapFlags(i32);

impl MmapFlags {
    /// Changes are private.
    pub const PRIVATE: Self = Self(MAP_PRIVATE);
    /// Allocate zero-filled memory not backed by a file.
    pub const ANONYMOUS: Self = Self(MAP_ANONYMOUS);

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
pub fn mprotect(ptr: NonNull<u8>, length: usize, prot: MmapProt) -> Result<(), String> {
    if unsafe { sys_mprotect(ptr.as_ptr().cast(), length, prot.bits()) } == -1 {
        return Err(os_error("mprotect"));
    }

    Ok(())
}

#[inline(always)]
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
