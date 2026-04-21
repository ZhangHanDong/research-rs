//! In-process local file reader. The v3 counterpart to `postagent` /
//! `browser` — no subprocess, no HTTP, just `std::fs::read`. Gated by
//! `route::classify_as_local` picking `Executor::Local`.
//!
//! Responsibilities:
//! - Read a single file off disk by absolute path.
//! - Enforce a per-file byte cap (default 256 KB per spec) so a stray
//!   binary or huge log doesn't nuke the session.
//! - Return raw bytes + a shape that plugs into the existing smell-test
//!   pipeline (the smell layer doesn't know we read locally).
//!
//! Directory walks live in a separate step (`add-local` command). This
//! module handles one path at a time.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Per-file byte cap. Matches the v3 spec default.
pub const DEFAULT_MAX_FILE_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone)]
pub struct LocalRead {
    /// File contents as bytes (empty on any error).
    pub body: Vec<u8>,
    /// Absolute path we actually read — echoed as `observed_url` so the
    /// session.jsonl record matches what the user asked for.
    pub observed_path: PathBuf,
    /// Wall-clock duration (mostly for parity with subprocess fetches).
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub enum LocalError {
    /// Path doesn't exist / can't stat / isn't readable.
    NotReadable(String),
    /// File is larger than `max_bytes`. `bytes` is the file's actual size.
    TooLarge { bytes: u64, cap: u64 },
    /// Caller passed a directory — this module only handles files. Dir
    /// walking happens one level up.
    IsDirectory,
    /// Non-UTF8 binary content that we won't try to snippet in a prompt.
    /// Accept bytes but flag via this variant; caller decides whether to
    /// reject on binary or keep as raw blob.
    Binary(PathBuf),
}

impl std::fmt::Display for LocalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LocalError::NotReadable(m) => write!(f, "local_not_readable: {m}"),
            LocalError::TooLarge { bytes, cap } => {
                write!(f, "local_too_large: {bytes} > {cap} cap")
            }
            LocalError::IsDirectory => write!(f, "local_is_directory"),
            LocalError::Binary(p) => write!(f, "local_binary: {}", p.display()),
        }
    }
}

/// Read a single file by absolute path. Returns the raw bytes and
/// bookkeeping needed by the calling add pipeline.
///
/// - `max_bytes`: per-file hard cap (use DEFAULT_MAX_FILE_BYTES when in
///   doubt). Files above cap return TooLarge without reading the body.
pub fn read_file(path: &Path, max_bytes: u64) -> Result<LocalRead, LocalError> {
    let start = Instant::now();
    let meta = fs::metadata(path)
        .map_err(|e| LocalError::NotReadable(format!("stat: {e}")))?;
    if meta.is_dir() {
        return Err(LocalError::IsDirectory);
    }
    let size = meta.len();
    if size > max_bytes {
        return Err(LocalError::TooLarge {
            bytes: size,
            cap: max_bytes,
        });
    }
    let body = fs::read(path).map_err(|e| LocalError::NotReadable(format!("read: {e}")))?;
    Ok(LocalRead {
        body,
        observed_path: path.to_path_buf(),
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

/// True if `path` looks like a text file by its first 1 KB (no NUL
/// bytes and mostly printable ASCII / valid UTF-8). Used to steer
/// binaries out of the ingest queue.
pub fn looks_like_text(bytes: &[u8]) -> bool {
    let probe = &bytes[..bytes.len().min(1024)];
    if probe.contains(&0u8) {
        return false;
    }
    let ascii_printable = probe
        .iter()
        .filter(|&&b| b == b'\n' || b == b'\r' || b == b'\t' || (0x20..=0x7e).contains(&b))
        .count();
    // Over 85% printable → treat as text. Leaves room for UTF-8 bytes.
    (ascii_printable * 100) >= (probe.len() * 85)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn read_file_happy_path() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp.as_file(), "hello local").unwrap();
        let r = read_file(tmp.path(), DEFAULT_MAX_FILE_BYTES).unwrap();
        assert!(r.body.starts_with(b"hello local"));
        assert_eq!(r.observed_path, tmp.path());
    }

    #[test]
    fn read_file_missing_returns_not_readable() {
        let missing = std::path::Path::new("/tmp/definitely/not/a/path/xyz-123");
        match read_file(missing, DEFAULT_MAX_FILE_BYTES) {
            Err(LocalError::NotReadable(_)) => {}
            other => panic!("expected NotReadable, got {other:?}"),
        }
    }

    #[test]
    fn read_file_rejects_oversize() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        let big = vec![b'x'; 2048];
        tmp.write_all(&big).unwrap();
        match read_file(tmp.path(), 1024) {
            Err(LocalError::TooLarge { bytes, cap }) => {
                assert_eq!(bytes, 2048);
                assert_eq!(cap, 1024);
            }
            other => panic!("expected TooLarge, got {other:?}"),
        }
    }

    #[test]
    fn read_file_rejects_directory() {
        let tmp = tempfile::tempdir().unwrap();
        match read_file(tmp.path(), DEFAULT_MAX_FILE_BYTES) {
            Err(LocalError::IsDirectory) => {}
            other => panic!("expected IsDirectory, got {other:?}"),
        }
    }

    #[test]
    fn looks_like_text_accepts_plain_ascii() {
        assert!(looks_like_text(b"fn main() { println!(\"hello\"); }"));
    }

    #[test]
    fn looks_like_text_rejects_null_bytes() {
        let mut v = b"prefix".to_vec();
        v.push(0);
        v.extend_from_slice(b"suffix");
        assert!(!looks_like_text(&v));
    }
}
