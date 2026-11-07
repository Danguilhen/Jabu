//! Best-effort single-instance guard.
//!
//! On Unix the first process binds a socket in the runtime directory. A later
//! process connects and exits, so it cannot open a second main window. The
//! socket is removed when the primary process exits.

use anyhow::Result;
#[cfg(unix)]
use std::io::Write;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(unix)]
use std::path::PathBuf;

/// Keeps the single-instance socket alive for the process lifetime.
#[cfg(unix)]
pub struct SingleInstanceGuard {
    path: PathBuf,
}

#[cfg(unix)]
impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// A no-op guard on platforms without the Unix socket implementation.
#[cfg(not(unix))]
pub struct SingleInstanceGuard;

/// Try to become the primary instance.
///
/// Returns `Some(guard)` for the first process and `None` when another
/// instance already owns the socket. The guard must stay alive for as long as
/// the process runs.
#[cfg(unix)]
pub fn acquire(name: &str) -> Result<Option<SingleInstanceGuard>> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let path = dir.join(format!("{name}.sock"));

    match UnixListener::bind(&path) {
        Ok(listener) => {
            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let mut stream = stream;
                    let _ = stream.write_all(b"raise\n");
                }
            });
            Ok(Some(SingleInstanceGuard { path }))
        }
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            if UnixStream::connect(&path).is_ok() {
                return Ok(None);
            }
            // A previous process crashed and left the socket behind.
            let _ = std::fs::remove_file(&path);
            match UnixListener::bind(&path) {
                Ok(listener) => {
                    std::thread::spawn(move || {
                        for stream in listener.incoming().flatten() {
                            let mut stream = stream;
                            let _ = stream.write_all(b"raise\n");
                        }
                    });
                    Ok(Some(SingleInstanceGuard { path }))
                }
                Err(error) => Err(error.into()),
            }
        }
        Err(error) => Err(error.into()),
    }
}

/// Other platforms currently allow multiple instances.
#[cfg(not(unix))]
pub fn acquire(_name: &str) -> Result<Option<SingleInstanceGuard>> {
    Ok(Some(SingleInstanceGuard))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn a_second_acquire_is_refused_until_the_guard_drops() {
        let name = format!(
            "jabu-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let first = acquire(&name).expect("first acquire").expect("primary");
        assert!(acquire(&name).expect("second acquire").is_none());
        drop(first);
        assert!(acquire(&name).expect("acquire after drop").is_some());
    }
}
