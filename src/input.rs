use crate::mmap;
use std::fs::File;
use std::os::fd::AsRawFd;

/// Dealing with different input sources efficiently and in a unified way, by for instance memory
/// mapping file inputs on x86 linux
pub enum Input {
    Str(String),
    File(Vec<u8>),
    MmapedFile {
        /// this needs to be kept so its not dropped while reading
        file: File,
        len: usize,
        ptr: std::ptr::NonNull<u8>,
    },
}

impl Input {
    pub fn from_file(file_name: &str) -> Self {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            let file = File::open(file_name).expect("Failed to open file");
            let meta = file.metadata().expect("Failed to get metadata");
            let len = meta.len() as usize;
            let ptr = mmap::mmap(
                None,
                len,
                mmap::MmapProt::READ,
                mmap::MmapFlags::PRIVATE,
                file.as_raw_fd(),
                0,
            )
            .expect("Failed to memory map file");
            crate::trace!("mmaped the file");
            Self::MmapedFile { file, len, ptr }
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            let mut file = File::open(file_name).expect("Failed to open file");
            let meta = file.metadata().expect("Failed to get metadata");
            let len = meta.len() as usize;
            let mut buf = Vec::with_capacity(len);
            std::io::Read::read_to_end(&mut file, &mut buf).expect("Failed to read file");
            Self::File(buf)
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Input::Str(s) => s.as_bytes(),
            Input::File(buf) => &buf,
            Input::MmapedFile { file, len, ptr } => unsafe {
                std::slice::from_raw_parts(ptr.as_ptr(), *len)
            },
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Input::Str(s) => &s,
            Input::File(buf) => str::from_utf8(&buf).unwrap(),
            Input::MmapedFile { file, len, ptr } => unsafe {
                str::from_utf8(std::slice::from_raw_parts(ptr.as_ptr(), *len)).unwrap()
            },
        }
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        match self {
            Input::MmapedFile { file, len, ptr } => {
                let cpy = *ptr;
                mmap::munmap(cpy, *len).expect("Failed to unmap file");
            }
            _ => (),
        }
    }
}
