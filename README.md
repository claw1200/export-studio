# ExportStudio - The Export Mode You Actually Want

ExportStudio is a library browser for rekordbox. Browse your collection, preview tracks, edit playlists and tags, analyse new files, and copy playlists to a USB stick. Your library stays in rekordbox's own database.

> [!NOTE]
> ExportStudio opens a rekordbox 6 or 7 library in read-only mode. Close rekordbox, then click the lock in the title bar to edit.

## Key Features

ExportStudio's goal is not just to strip down Rekordbox, but improve on the feature set. ExportStudio improves:

### UI

Collection, playlists, histories, tags, and devices in one responsive window.

![The collection, playlists, and deck in one window.](preview/images/full_app.webp)

### High-resolution waveforms

ExportStudio adds custom high-resolution waveforms, drawn from the audio, at up to 16,000 samples a second. This lets you zoom in on the waveform in far greater detail than rekordbox's built-in waveforms, so you can be confident your beatgrid is accurate.

A quality slider changes the number of samples used to draw the waveform. Off uses the waveform rekordbox stored during analysis.

<img src="preview/images/highres_waveform.webp" alt="A zoomed waveform drawn from the audio." height="480">

### Grid editing

Shift the beatgrid onto the kick, a millisecond at a time, or halve and double the tempo. Save writes it back to rekordbox.

<img src="preview/images/grid_edit.webp" alt="Editing a beatgrid on the deck, with nudge, half, and double." height="480">

### Analysis

ExportStudio works out tempo, beatgrid, and key itself. A normal grid fits one tempo to the whole track. Dynamic follows tempo changes, for live recordings and tracks that speed up. Cues are kept.

Tested on vast rekordbox libraries, it almost always matches or improves on rekordbox's analysis for beatgrids, tempo, and key.

<img src="preview/images/analysis.webp" alt="Analysis settings: grid type, BPM range, precision, and key detection." height="480">

### USB sync

Playlists you pick are checked before they replace what is on the stick. Playlists, history, and tracks this app did not create are left in place.

DeviceLibrary and OneLibrary are both written to the device, so that compatibility is preserved across all players.

<img src="preview/images/usb_sync.webp" alt="Choosing which playlists to copy onto a USB stick." height="480">

### Backups

Backups are run when you unlock editing, and once a day when something has changed. You can restore any backup from Settings, a backup is taken before you restore, so you can undo the restore if you made a mistake.

<img src="preview/images/backups.webp" alt="Saved library backups, with restore, in Settings." height="480">


### Editing

A snapshot until you unlock. Writes only while rekordbox is closed.

<img src="preview/images/editing_metadata.webp" alt="Editing a track's title, artist, rating, and colour." height="480">
