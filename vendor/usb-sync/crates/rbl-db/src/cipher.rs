//! Waiting for `SQLCipher` to finish its process-wide setup.
//!
//! `sqlite3_initialize` marks SQLite initialized and releases its lock before
//! it runs `SQLCipher`'s own setup (`sqlcipher_extra_init`: the private heap and
//! the crypto provider's random seed). A connection opened on another thread
//! in that window is keyed before `SQLCipher` is ready, and `PRAGMA key` fails
//! with "An error occurred with PRAGMA key or rekey". The tests that open
//! their libraries first in a busy test binary hit it (bundled `SQLCipher`
//! 4.14.0, `sqlite3.c` `sqlcipherCodecAttach` returning `sqlcipher_init_error`
//! while `sqlcipher_init` is still 0).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rusqlite::Connection;

static READY: AtomicBool = AtomicBool::new(false);

/// Returns once a key can be set on a connection in this process.
///
/// Keys a throwaway in-memory database until `SQLCipher` accepts it, so a
/// caller that raced another thread's first open waits for that open's setup
/// to finish instead of failing. Costs one atomic load after the first call.
pub fn ready() -> rusqlite::Result<()> {
    if READY.load(Ordering::Acquire) {
        return Ok(());
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let conn = Connection::open_in_memory()?;
        match conn.pragma_update(None, "key", "ready") {
            Ok(()) => {
                READY.store(true, Ordering::Release);
                return Ok(());
            }
            Err(error) if Instant::now() >= deadline => return Err(error),
            Err(_) => std::thread::sleep(Duration::from_millis(1)),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn many_threads_keying_at_once_all_succeed() {
        let threads: Vec<_> = (0..16)
            .map(|_| {
                std::thread::spawn(|| {
                    ready()?;
                    let conn = Connection::open_in_memory()?;
                    conn.pragma_update(None, "key", "secret")
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap().unwrap();
        }
    }
}
