//! Optional discovery conveniences; database operations take explicit paths.

use std::path::PathBuf;

pub fn installed_path() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(|directory| PathBuf::from(directory).join("Pioneer").join("rekordbox"));
    #[cfg(not(windows))]
    let base = std::env::var_os("HOME")
        .map(|directory| PathBuf::from(directory).join("Library").join("Pioneer").join("rekordbox"));
    base.map(|directory| directory.join("master.db"))
}