use std::path::Path;

use es_anlz::decode::{self, Tags};
use es_db::fixture;
use es_db::{CueMove, Guard, Library};
use es_engine::analysis::{apply_analysis, set_beatgrid, Analysis, Grid, Options};
use es_engine::import::{import_files, Options as ImportOptions};

fn wav(path: &Path) {
    let data = 16000u32;
    let mut bytes = b"RIFF".to_vec();
    bytes.extend((36 + data).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend([1, 0, 1, 0]);
    bytes.extend(8000u32.to_le_bytes());
    bytes.extend(16000u32.to_le_bytes());
    bytes.extend([2, 0, 16, 0]);
    bytes.extend(b"data");
    bytes.extend(data.to_le_bytes());
    bytes.extend(vec![0u8; data as usize]);
    std::fs::write(path, bytes).unwrap();
}

fn scene() -> (tempfile::TempDir, Library, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture::build(dir.path(), 5).unwrap();
    let mut lib = Library::open_live(&path, Guard::Off).unwrap();
    let share = lib.share_dir();
    let file = dir.path().join("Song.wav");
    wav(&file);
    let id = import_files(&mut lib, &share, &[file.to_string_lossy().into_owned()], &ImportOptions { artwork: false, playlist: None }).unwrap().ids[0].clone();
    (dir, lib, id)
}

fn analysis() -> Analysis {
    Analysis {
        beats: vec![(1, 12800, 0), (2, 12800, 469), (3, 12800, 938), (4, 12800, 1406)],
        waveform: (0..8 * 300).map(|i| (i * 7 % 251) as u8).collect(),
        key: Some((9, true)),
        bpm: 128.0,
        duration: 2.0,
    }
}

const ALL: Options = Options { overwrite: false, grid: true, key: true };

#[test]
fn an_analysis_creates_files_and_fills_the_row() {
    let (_dir, mut lib, id) = scene();
    let share = lib.share_dir();
    apply_analysis(&mut lib, &share, &id, &analysis(), ALL).unwrap();

    let files = lib.track_files(&id).unwrap().unwrap();
    let relative = files.analysis_path.expect("an analysis path is recorded");
    let dat = Tags::read(&es_anlz::file(&share, &relative, "DAT").unwrap()).unwrap();
    assert_eq!(decode::beatgrid(&dat).unwrap().times, vec![0, 469, 938, 1406]);
    let ext = Tags::read(&es_anlz::file(&share, &relative, "EXT").unwrap()).unwrap();
    assert_eq!(decode::rgb(&ext).unwrap().length, 300);
    let row = lib.tracks(Some(std::slice::from_ref(&id))).unwrap().remove(0);
    assert_eq!(row.bpm_x100, 12800);
    assert_eq!(row.key, "8A");
}

#[test]
fn an_existing_grid_survives_unless_overwrite_is_asked_for() {
    let (_dir, mut lib, id) = scene();
    let share = lib.share_dir();
    apply_analysis(&mut lib, &share, &id, &analysis(), ALL).unwrap();
    let mut other = analysis();
    other.beats = vec![(1, 9000, 0), (2, 9000, 667)];
    other.bpm = 90.0;
    apply_analysis(&mut lib, &share, &id, &other, ALL).unwrap();
    assert_eq!(lib.tracks(Some(std::slice::from_ref(&id))).unwrap().remove(0).bpm_x100, 12800);
    apply_analysis(&mut lib, &share, &id, &other, Options { overwrite: true, ..ALL }).unwrap();
    assert_eq!(lib.tracks(Some(std::slice::from_ref(&id))).unwrap().remove(0).bpm_x100, 9000);
}

#[test]
fn a_hand_edited_grid_is_written_and_moves_cues() {
    let (_dir, mut lib, id) = scene();
    let share = lib.share_dir();
    apply_analysis(&mut lib, &share, &id, &analysis(), ALL).unwrap();
    lib.write(|tx| tx.set_hot_cue(&id, 1, 500)).unwrap();
    let cue = lib.cues(&id).unwrap().remove(0);

    let grid = Grid { times: vec![10, 500, 990], beats: vec![1, 2, 3], bpms: vec![122.0, 122.0, 122.0] };
    let moves = [CueMove { id: cue.id.clone(), start_ms: 510, end_ms: None }];
    let bpm = set_beatgrid(&mut lib, &share, &id, &grid, &moves).unwrap();
    assert_eq!(bpm, Some(122.0));

    let relative = lib.track_files(&id).unwrap().unwrap().analysis_path.unwrap();
    let dat = Tags::read(&es_anlz::file(&share, &relative, "DAT").unwrap()).unwrap();
    assert_eq!(decode::beatgrid(&dat).unwrap().times, vec![10, 500, 990]);
    assert_eq!(lib.cues(&id).unwrap().remove(0).start_ms, 510);
    assert_eq!(lib.tracks(Some(std::slice::from_ref(&id))).unwrap().remove(0).bpm_x100, 12200);
}

#[test]
fn bad_grids_are_refused() {
    let (_dir, mut lib, id) = scene();
    let share = lib.share_dir();
    for grid in [
        Grid { times: vec![], beats: vec![], bpms: vec![] },
        Grid { times: vec![5, 5], beats: vec![1, 2], bpms: vec![120.0, 120.0] },
        Grid { times: vec![0], beats: vec![5], bpms: vec![120.0] },
        Grid { times: vec![0], beats: vec![1], bpms: vec![10.0] },
    ] {
        assert!(set_beatgrid(&mut lib, &share, &id, &grid, &[]).is_err());
    }
    let ok = Grid { times: vec![0], beats: vec![1], bpms: vec![120.0] };
    assert!(set_beatgrid(&mut lib, &share, &id, &ok, &[]).unwrap_err().to_string().contains("no analysis file"));
}
