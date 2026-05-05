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
    pub fn from_file(file_name: &str) -> Result<Self, String> {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            let file = File::open(file_name)
                .map_err(|e| format!("Failed to open file '{file_name}': {e}"))?;
            let meta = file
                .metadata()
                .map_err(|e| format!("Failed to read metadata for '{file_name}': {e}"))?;
            let len = meta.len() as usize;
            if len == 0 {
                return Ok(Input::Str(String::new()));
            }

            let ptr = mmap::mmap(
                None,
                len,
                mmap::MmapProt::READ,
                mmap::MmapFlags::PRIVATE,
                file.as_raw_fd(),
                0,
            )
            .map_err(|e| format!("Failed to memory map '{file_name}': {e}"))?;
            crate::trace!("[input::Input::from_file] mmaped the file");
            Ok(Self::MmapedFile { file, len, ptr })
        }

        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        {
            let mut file = File::open(file_name)
                .map_err(|e| format!("Failed to open file '{file_name}': {e}"))?;
            let meta = file
                .metadata()
                .map_err(|e| format!("Failed to read metadata for '{file_name}': {e}"))?;
            let len = meta.len() as usize;
            if len == 0 {
                return Ok(Input::Str(String::new()));
            }
            let mut buf = Vec::with_capacity(len);
            std::io::Read::read_to_end(&mut file, &mut buf)
                .map_err(|e| format!("Failed to read file '{file_name}': {e}"))?;
            Ok(Self::File(buf))
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

    pub fn size(&self) -> usize {
        match self {
            Input::Str(s) => s.len(),
            Input::File(bytes) => bytes.len(),
            Input::MmapedFile { len, .. } => *len,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.size() == 0
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
