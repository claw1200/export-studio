//! Where rekordbox keeps its own files on this machine.

use std::path::PathBuf;

/// rekordbox's settings directory: `rekordbox3.settings`, and beside it the
/// `MYSETTING.DAT`, `MYSETTING2.DAT`, `DJMMYSETTING.DAT` and `djprofile.nxs`
/// it copies to every stick it exports to. `~/Library/Application
/// Support/Pioneer/rekordbox6` on macOS, `%APPDATA%\\Pioneer\\rekordbox6`
/// on Windows [OBS 7.2.11]. `None` when rekordbox is not installed here.
#[must_use]
pub fn rekordbox_settings_dir() -> Option<PathBuf> {
    let base = if cfg!(target_os = "macos") {
        dirs::home_dir()?.join("Library/Application Support")
    } else {
        dirs::config_dir()?
    };
    let dir = base.join("Pioneer/rekordbox6");
    dir.is_dir().then_some(dir)
}
