use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::Error;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_path(path: &Path) -> PathBuf {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(".{name}.{}.{unique}.tmp", std::process::id()))
}

#[cfg(windows)]
fn rename_replace(from: &Path, to: &Path) -> io::Result<()> {
    let mut attempt = 0;
    loop {
        match fs::rename(from, to) {
            Err(err) if err.kind() == io::ErrorKind::PermissionDenied && attempt < 10 => {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            result => return result,
        }
    }
}

#[cfg(not(windows))]
fn rename_replace(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

fn write_temp(tmp: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(tmp)?;
    file.write_all(bytes)?;
    file.sync_all()
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = temp_path(path);
    let result = write_temp(&tmp, bytes).and_then(|()| rename_replace(&tmp, path));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// The lock file is never deleted: removing it would let two processes lock different inodes.
fn lock_exclusive(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut name: OsString = path.as_os_str().to_owned();
    name.push(".lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(PathBuf::from(name))?;
    file.lock()?;
    Ok(file)
}

/// `Ok(None)` when the file does not exist; never reads more than `max_bytes + 1` bytes.
fn read_bounded(path: &Path, max_bytes: u64) -> Result<Option<Vec<u8>>, Error> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    let too_big = || {
        Error::Runtime(format!(
            "{} is larger than {max_bytes} bytes",
            path.display()
        ))
    };
    if file.metadata()?.len() > max_bytes {
        return Err(too_big());
    }
    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(too_big());
    }
    Ok(Some(bytes))
}

/// Reads `path` (at most `max_bytes`, else `Err` with the file untouched), passes it to `f`
/// and writes the result atomically, all under the file's lock.
pub fn update_json<T: DeserializeOwned + Serialize>(
    path: &Path,
    max_bytes: u64,
    f: impl FnOnce(Option<T>) -> Result<T, Error>,
) -> Result<T, Error> {
    let _lock = lock_exclusive(path)?;
    let current = match read_bounded(path, max_bytes)? {
        Some(bytes) => Some(serde_json::from_slice(&bytes)?),
        None => None,
    };
    let updated = f(current)?;
    let mut text = serde_json::to_string_pretty(&updated)?;
    text.push('\n');
    write_atomic(path, text.as_bytes())?;
    Ok(updated)
}
