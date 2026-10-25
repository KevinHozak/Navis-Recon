//! Cooperative cross-process CAS for local JSON documents. Never touches ledger amounts.
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
};
pub(crate) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn lock(path: &Path) -> Result<File, String> {
    let parent = path.parent().ok_or("Document has no parent")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let lock = path.with_extension("json.lock");
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    #[cfg(not(windows))]
    return Err("Document saves require Windows exclusive locking".into());
    #[cfg(windows)]
    options
        .open(lock)
        .map_err(|_| "Document is busy; refresh and retry".into())
}
pub(crate) fn read(path: &Path) -> Result<(Vec<u8>, String), String> {
    match fs::read(path) {
        Ok(bytes) => {
            let revision = hash(&bytes);
            Ok((bytes, revision))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((Vec::new(), "missing".into())),
        Err(e) => Err(format!("Document unavailable: {e}")),
    }
}
pub(crate) fn save(path: &Path, expected: &str, bytes: &[u8]) -> Result<String, String> {
    // Caller retains the sidecar lock from read/validation until readback finishes.
    if read(path)?.1 != expected {
        return Err("Document conflict: refresh before saving; external changes preserved".into());
    }
    let stage = path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let result = (|| {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stage)
            .map_err(|e| e.to_string())?;
        f.write_all(bytes)
            .and_then(|_| f.sync_all())
            .map_err(|e| e.to_string())?;
        drop(f);
        if read(path)?.1 != expected {
            return Err("Document conflict: refresh before saving".into());
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Storage::FileSystem::{
                MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
            };
            let from: Vec<u16> = stage.as_os_str().encode_wide().chain(Some(0)).collect();
            let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            if unsafe {
                MoveFileExW(
                    from.as_ptr(),
                    to.as_ptr(),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            } == 0
            {
                return Err(format!(
                    "Document publication failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
        }
        #[cfg(not(windows))]
        return Err("Document saves require Windows".into());
        let (observed, revision) = read(path)?;
        if observed != bytes {
            return Err("Document readback differs; refresh before retrying".into());
        }
        Ok(revision)
    })();
    if stage.exists() {
        let _ = fs::remove_file(&stage);
    }
    result
}
