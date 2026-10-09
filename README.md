# ExportStudio - The Export Mode You Actually Want

ExportStudio is a library browser for rekordbox. Browse your collection, preview tracks, edit playlists and tags, analyse new files, and copy playlists to a USB stick. Your library stays in rekordbox's own database.

> [!NOTE]
> ExportStudio opens a rekordbox 6 or 7 library in read-only mode. Close rekordbox, then click the lock in the title bar to edit.


|                   | ExportStudio                                                                                                                                                        | rekordbox                                                             |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| Library and USB   | Collection, playlists, histories, tags, and devices in one window.                                                                                                  | Collection in Performance mode. USB export in a separate Export mode. |
| Editing           | A snapshot until you unlock. Writes only while rekordbox is closed.                                                                                                 | The library is live for as long as rekordbox is open.                 |
| Track list        | A waveform on each row. Click or scrub it to hear that spot, without moving the track on the deck.                                                                  | No waveform in the list that you can play.                            |
| Preparing a track | Scrub, quantise, cues, loops, tempo, and beatgrid edits on the deck.                                                                                                | The deck is in Performance mode, not in Export mode.                  |
| Waveform detail   | Can be drawn from the audio, at a quality you set. The rekordbox waveform is still there when that is off.                                                          | The waveform stored by analysis.                                      |
| Analysis          | Its own tempo, beatgrid, key, and waveform colour. Normal or dynamic grid. Cues are kept.                                                                           | rekordbox's own analysis.                                             |
| USB sync          | Incremental. The playlists you pick are checked before they replace what is on the stick. Playlists, history, and tracks this app did not create are left in place. | Sync the playlists you pick from Export mode.                         |
| Streaming         | Streaming tracks can be hidden from every list. They stay in the rekordbox library.                                                                                 | Streaming tracks stay mixed into the collection and playlists.        |
| Backups           | Before the first edit after you unlock, and once a day when something has changed. Restore from Settings.                                                           | A manual library backup.                                              |


## Edit

While the library is locked, you are looking at a snapshot. Unlocking writes to the rekordbox database. If rekordbox is still open, the lock stays closed.

- Change title, artist, and the other text fields in the info panel, or edit several tracks at once from the right-click menu.
- Set colour, rating, and My Tags from that same menu.
- Drag tracks onto a playlist or a tag. Right-click the playlist list to create a playlist or a folder.
- Drop audio files on the collection or a playlist to import them, or use **Import** in the title bar.
- Missing files are marked in the list. Right-click one to relocate it, or auto-relocate a selection.
- **Remove from playlist** leaves the track in your collection. **Remove from collection** takes it out of the library.

> [!TIP]
> **Settings > Database & files** can also write title, artist, album, and the other text fields into the audio file's own tags. Rating and colour always stay in rekordbox.

## Analyse

ExportStudio works out tempo, beatgrid, key, and waveform colour itself. This is separate from rekordbox's analysis. Cues are kept when a track is analysed again.

Turn on **Analyse imported tracks** under **Settings > Import & analysis**, or analyse a selection from the track menu. The same page chooses a normal or dynamic grid, the BPM range, and how many tracks to analyse at once.

## USB

A stick that rekordbox has exported to shows under **Devices**, whether it uses the older Device Library or OneLibrary (Device Library Plus). When a stick has both, ExportStudio reads OneLibrary. Open a device to browse the playlists on that volume.

Right-click a playlist and choose **Sync to...** to copy the playlists you tick onto a volume. A sync writes both formats, so older players and newer ones can read the stick.

## Backups

**Settings > Database & files** lists saved backups. With automatic backups on, ExportStudio saves one before the first edit after you unlock, and once a day, when something has changed. Manual backups stay until you delete them. Close rekordbox before you restore one.

> [!WARNING]
> A backup holds the rekordbox database, playlists, cues, beatgrids, waveforms, and artwork. It does not include your audio files.

## Developing

You need Node.js, a Rust toolchain, and [uv](https://docs.astral.sh/uv/) with Python 3.12. The library engine is started with `uv` when you run the app.

```sh
cd app
npm install
npm run dev
```


| Command               | What it does                                |
| --------------------- | ------------------------------------------- |
| `npm run dev`         | Builds the audio addon, then starts the app |
| `npm run typecheck`   | Type-checks the Electron app                |
| `npm run test:native` | Runs the Rust tests                         |
| `npm run build:mac`   | Packages a macOS app                        |
| `npm run build:win`   | Packages a Windows app                      |



| Path      | What lives there                                                                                      |
| --------- | ----------------------------------------------------------------------------------------------------- |
| `app/`    | Electron and Svelte interface                                                                         |
| `engine/` | Python library engine. Reads and writes the rekordbox database.                                       |
| `native/` | Rust audio engine for playback, scrub, loops, and analysis. See [native/README.md](native/README.md). |
