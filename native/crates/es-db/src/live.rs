//! What "live" means. Every read and count builds on these, so a list and its count cannot disagree.

/// The playlist the cloud sync keeps; it is never shown.
pub const HIDDEN_PLAYLIST: &str = "100000";

pub const TRACK_LIVE: &str = "c.rb_local_deleted = 0";

/// A playlist entry counts only when the entry, its track and its playlist are all live.
pub const MEMBERS_FROM: &str = "djmdSongPlaylist sp \
     JOIN djmdContent c ON c.ID = sp.ContentID \
     JOIN djmdPlaylist p ON p.ID = sp.PlaylistID";

pub const MEMBERS_WHERE: &str =
    "sp.rb_local_deleted = 0 AND c.rb_local_deleted = 0 AND p.rb_local_deleted = 0";
