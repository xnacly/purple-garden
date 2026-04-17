//! copied and applied from https://github.com/xnacly/stinkarm/blob/master/src/mem/mmap.rs

const MMAP_SYSCALL: i64 = 9;
const MPROTECT_SYSCALL: i64 = 10;
const MUNMAP_SYSCALL: i64 = 11;

// Not an enum, since NONE, READ, WRITE and EXEC arent mutually exclusive
pub struct MmapProt(i32);
impl MmapProt {
    /// no permissions
    pub const NONE: MmapProt = MmapProt(0x00);
    /// pages can be read
    pub const READ: MmapProt = MmapProt(0x01);
    /// pages can be written
    pub const WRITE: MmapProt = MmapProt(0x02);
    /// pages can be executed
    pub const EXEC: MmapProt = MmapProt(0x04);
    pub fn bits(self) -> i32 {
        self.0
    }
}

impl std::ops::BitOr for MmapProt {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        MmapProt(self.0 | rhs.0)
    }
}

pub struct MmapFlags(i32);

impl MmapFlags {
    /// share changes
    pub const SHARED: MmapFlags = MmapFlags(0x0001);
    /// changes are private
    pub const PRIVATE: MmapFlags = MmapFlags(0x0002);
    /// map addr must be exactly as requested
    pub const FIXED: MmapFlags = MmapFlags(0x0010);

    // MAP_FIXED_NOREPLACE (Linux ≥ 5.4)
    pub const NOREPLACE: MmapFlags = MmapFlags(0x100000);

    /// allocated from memory, swap space
    pub const ANONYMOUS: MmapFlags = MmapFlags(0x20);

    /// mapping is used for stack
    pub const STACK: MmapFlags = MmapFlags(0x4000);

    /// omit from dumps
    pub const CONCEAL: MmapFlags = MmapFlags(0x8000);

    pub fn bits(self) -> i32 {
        self.0
    }
}

impl std::ops::BitOr for MmapFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        MmapFlags(self.0 | rhs.0)
    }
}

#[inline(always)]
pub fn mmap(
    ptr: Option<std::ptr::NonNull<u8>>,
    length: usize,
    prot: MmapProt,
    flags: MmapFlags,
    fd: i32,
    offset: i64,
) -> Result<std::ptr::NonNull<u8>, String> {
    let ret: isize;

    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") MMAP_SYSCALL,
            in("rdi") ptr.map(|nn| nn.as_ptr()).unwrap_or(std::ptr::null_mut()),
            in("rsi") length,
            in("rdx") prot.bits(),
            in("r10") flags.bits(),
            in("r8")  fd,
            in("r9")  offset,
            lateout("rax") ret,
            clobber_abi("sysv64"),
            options(nostack)
        );
    }
    if ret < 0 {
        let errno = -ret;
        return Err(format!(
            "mmap failed (errno {}): {}",
            errno,
            std::io::Error::from_raw_os_error(errno as i32)
        ));
    }

    Ok(unsafe { std::ptr::NonNull::new_unchecked(ret as *mut u8) })
}

#[inline(always)]
pub fn munmap(ptr: std::ptr::NonNull<u8>, size: usize) -> Result<(), String> {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") MUNMAP_SYSCALL,
            in("rdi") ptr.as_ptr(),
            in("rsi") size,
            lateout("rax") ret,
            clobber_abi("sysv64"),
            options(nostack)
        );
    }

    if ret < 0 {
        let errno = -ret;
        return Err(format!(
            "munmap failed (errno {}): {}",
            errno,
            std::io::Error::from_raw_os_error(errno as i32)
        ));
    }

    Ok(())
}
