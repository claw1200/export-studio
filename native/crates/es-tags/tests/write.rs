use es_tags::{read, write, Edit};

fn wav() -> Vec<u8> {
    let data = 16000u32;
    let mut b = b"RIFF".to_vec();
    b.extend((36 + data).to_le_bytes());
    b.extend(b"WAVEfmt ");
    b.extend(16u32.to_le_bytes());
    b.extend([1, 0, 1, 0]);
    b.extend(8000u32.to_le_bytes());
    b.extend(16000u32.to_le_bytes());
    b.extend([2, 0, 16, 0]);
    b.extend(b"data");
    b.extend(data.to_le_bytes());
    b.extend(vec![0u8; data as usize]);
    b
}

#[test]
fn every_field_round_trips_through_a_wav_and_repeats_do_not_stack_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.wav");
    std::fs::write(&path, wav()).unwrap();
    let edit = Edit {
        title: Some("T".into()),
        artist: Some("A".into()),
        album: Some("Al".into()),
        genre: Some("G".into()),
        label: Some("L".into()),
        remixer: Some("R".into()),
        original_artist: Some("O".into()),
        composer: Some("C".into()),
        comment: Some("note".into()),
        year: Some(2020),
        track_no: Some(3),
    };
    write(&path, &edit).unwrap();
    write(&path, &edit).unwrap();
    let f = read(&path).unwrap().fields;
    assert_eq!((f.title.as_str(), f.original_artist.as_str(), f.comment.as_str(), f.year, f.track_no), ("T", "O", "note", 2020, 3));
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(bytes.windows(4).filter(|w| w == b"ID3 " || w == b"id3 ").count(), 1);

    write(&path, &Edit { comment: Some(String::new()), year: Some(0), ..Edit::default() }).unwrap();
    let f = read(&path).unwrap().fields;
    assert_eq!((f.comment.as_str(), f.year, f.title.as_str()), ("", 0, "T"));
}

#[test]
fn missing_and_unsupported_files_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    assert!(write(&dir.path().join("none.mp3"), &Edit::default()).is_err());
    let junk = dir.path().join("a.txt");
    std::fs::write(&junk, "hello").unwrap();
    assert!(write(&junk, &Edit::default()).is_err());
}
