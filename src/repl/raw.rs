use std::io;

#[cfg(unix)]
use std::os::fd::AsRawFd;

#[cfg(unix)]
pub(super) struct RawMode {
    fd: i32,
    original: Termios,
}

#[cfg(unix)]
impl RawMode {
    pub(super) fn enter() -> io::Result<Self> {
        let fd = io::stdin().as_raw_fd();
        let original = read_termios(fd)?;
        let mut raw = original;
        raw.c_lflag &= !(ECHO | ICANON | IEXTEN | ISIG);
        raw.c_iflag &= !(BRKINT | ICRNL | INPCK | ISTRIP | IXON);
        raw.c_cflag |= CS8;
        raw.c_oflag &= !OPOST;
        raw.c_cc[VMIN] = 1;
        raw.c_cc[VTIME] = 0;
        write_termios(fd, TCSAFLUSH, &raw)?;
        Ok(Self { fd, original })
    }
}

#[cfg(unix)]
impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = write_termios(self.fd, TCSAFLUSH, &self.original);
    }
}

#[cfg(not(unix))]
pub(super) struct RawMode;

#[cfg(not(unix))]
impl RawMode {
    pub(super) fn enter() -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "raw terminal mode is only implemented on unix",
        ))
    }
}

#[cfg(all(unix, target_os = "macos"))]
#[repr(C)]
#[derive(Clone, Copy)]
struct Termios {
    c_iflag: u64,
    c_oflag: u64,
    c_cflag: u64,
    c_lflag: u64,
    c_cc: [u8; 20],
    c_ispeed: u64,
    c_ospeed: u64,
}

#[cfg(all(unix, target_os = "linux"))]
#[repr(C)]
#[derive(Clone, Copy)]
struct Termios {
    c_iflag: u32,
    c_oflag: u32,
    c_cflag: u32,
    c_lflag: u32,
    c_line: u8,
    c_cc: [u8; 32],
    c_ispeed: u32,
    c_ospeed: u32,
}

#[cfg(all(unix, target_os = "macos"))]
const TCSAFLUSH: i32 = 2;
#[cfg(all(unix, target_os = "linux"))]
const TCSAFLUSH: i32 = 2;
#[cfg(all(unix, target_os = "macos"))]
const VMIN: usize = 16;
#[cfg(all(unix, target_os = "macos"))]
const VTIME: usize = 17;
#[cfg(all(unix, target_os = "linux"))]
const VMIN: usize = 6;
#[cfg(all(unix, target_os = "linux"))]
const VTIME: usize = 5;

#[cfg(all(unix, target_os = "macos"))]
const BRKINT: u64 = 0x0002;
#[cfg(all(unix, target_os = "macos"))]
const ICRNL: u64 = 0x0100;
#[cfg(all(unix, target_os = "macos"))]
const INPCK: u64 = 0x0010;
#[cfg(all(unix, target_os = "macos"))]
const ISTRIP: u64 = 0x0020;
#[cfg(all(unix, target_os = "macos"))]
const IXON: u64 = 0x0200;
#[cfg(all(unix, target_os = "macos"))]
const OPOST: u64 = 0x0001;
#[cfg(all(unix, target_os = "macos"))]
const CS8: u64 = 0x0300;
#[cfg(all(unix, target_os = "macos"))]
const ECHO: u64 = 0x0008;
#[cfg(all(unix, target_os = "macos"))]
const ICANON: u64 = 0x0100;
#[cfg(all(unix, target_os = "macos"))]
const IEXTEN: u64 = 0x0400;
#[cfg(all(unix, target_os = "macos"))]
const ISIG: u64 = 0x0080;

#[cfg(all(unix, target_os = "linux"))]
const BRKINT: u32 = 0x0002;
#[cfg(all(unix, target_os = "linux"))]
const ICRNL: u32 = 0x0100;
#[cfg(all(unix, target_os = "linux"))]
const INPCK: u32 = 0x0010;
#[cfg(all(unix, target_os = "linux"))]
const ISTRIP: u32 = 0x0020;
#[cfg(all(unix, target_os = "linux"))]
const IXON: u32 = 0x0400;
#[cfg(all(unix, target_os = "linux"))]
const OPOST: u32 = 0x0001;
#[cfg(all(unix, target_os = "linux"))]
const CS8: u32 = 0x0030;
#[cfg(all(unix, target_os = "linux"))]
const ECHO: u32 = 0x0008;
#[cfg(all(unix, target_os = "linux"))]
const ICANON: u32 = 0x0002;
#[cfg(all(unix, target_os = "linux"))]
const IEXTEN: u32 = 0x8000;
#[cfg(all(unix, target_os = "linux"))]
const ISIG: u32 = 0x0001;

#[cfg(unix)]
unsafe extern "C" {
    fn tcgetattr(fd: i32, termios_p: *mut Termios) -> i32;
    fn tcsetattr(fd: i32, optional_actions: i32, termios_p: *const Termios) -> i32;
}

#[cfg(unix)]
fn read_termios(fd: i32) -> io::Result<Termios> {
    let mut termios = std::mem::MaybeUninit::<Termios>::uninit();
    let rc = unsafe { tcgetattr(fd, termios.as_mut_ptr()) };
    if rc == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { termios.assume_init() })
    }
}

#[cfg(unix)]
fn write_termios(fd: i32, optional_actions: i32, termios: &Termios) -> io::Result<()> {
    let rc = unsafe { tcsetattr(fd, optional_actions, termios as *const Termios) };
    if rc == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
