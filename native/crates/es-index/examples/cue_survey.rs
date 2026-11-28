//! How a playlist's cues sit against the beat grid and rekordbox's phrases: what an automatic cue
//! placer would have to learn. Reads a snapshot of the installed library and changes nothing.
//! `cargo run --release -p es-index --example cue_survey -- "<playlist or folder>" [--json survey.json]`

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use rb_anlz::decode::{self, Grid, Tags};
use rb_anlz::phrase::{self, Mood, SongStructure};
use rb_library::{Library, PlaylistKind, PlaylistNode, Track};
use serde_json::json;

type Beats = i64;
type Bars = i64;

const BEATS_PER_BAR: Beats = 4;
const ON_BEAT_TOLERANCE_MS: i64 = 15;
const PHRASE_GRIDS: [Bars; 4] = [4, 8, 16, 32];
const TOP_ANCHORS: usize = 10;
const TOP_BOUNDARIES: usize = 20;

struct SurveyedTrack {
    track: Track,
    grid: Option<Grid>,
    structure: Option<SongStructure>,
    cues: Vec<es_index::detail::Cue>,
}

struct Placement {
    hot_slot: Option<char>,
    beat: usize,
    on_beat: bool,
    on_downbeat: bool,
    beats_from_first_downbeat: Beats,
    track_fraction: f64,
    anchor: Option<PhraseAnchor>,
}

struct PhraseAnchor {
    name: String,
    beats_after: Beats,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let wanted = args.next().ok_or("name a playlist or folder")?;
    let json_path = match (args.next().as_deref(), args.next()) {
        (Some("--json"), Some(path)) => Some(path),
        _ => None,
    };

    let path = Library::installed_path().ok_or("no installed rekordbox library")?;
    let lib = Library::open_snapshot(&path)?;
    let share = path.with_file_name("share");
    let ids = playlist_track_ids(&lib, &wanted)?;
    let tracks: Vec<SurveyedTrack> =
        lib.tracks(Some(&ids))?.into_iter().map(|track| survey(&lib, &share, track)).collect::<Result<_, _>>()?;

    if let Some(path) = json_path {
        std::fs::write(&path, serde_json::to_string_pretty(&tracks.iter().map(to_json).collect::<Vec<_>>())?)?;
        println!("wrote {path}");
    }
    report(&tracks);
    Ok(())
}

fn playlist_track_ids(lib: &Library, wanted: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let nodes = lib.playlists()?;
    let matches: Vec<&PlaylistNode> =
        nodes.iter().filter(|node| node.id == wanted || node.name.eq_ignore_ascii_case(wanted)).collect();
    let root = match matches.as_slice() {
        [one] => *one,
        [] => return Err(format!("no playlist or folder called {wanted}").into()),
        many => {
            let ids: Vec<_> = many.iter().map(|node| node.id.as_str()).collect();
            return Err(format!("{wanted} names several playlists; pass one id of {ids:?}").into());
        }
    };
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if node.kind == PlaylistKind::Folder {
            pending.extend(nodes.iter().filter(|child| child.parent_id == node.id));
            continue;
        }
        ids.extend(lib.playlist_track_ids(&node.id)?.into_iter().filter(|id| seen.insert(id.clone())));
    }
    Ok(ids)
}

fn survey(lib: &Library, share: &Path, track: Track) -> rb_library::Result<SurveyedTrack> {
    let analysis = |suffix| {
        let relative = track.analysis_path.as_deref()?;
        Tags::read(&rb_anlz::file(share, relative, suffix)?).ok()
    };
    let grid = analysis("DAT").as_ref().and_then(decode::beatgrid);
    let structure = analysis("EXT").as_ref().and_then(phrase::song_structure);
    let cues = es_index::detail::cues(lib, &track.id, None)?;
    Ok(SurveyedTrack { track, grid, structure, cues })
}

fn hot_cue_letter(slot: i64) -> Option<char> {
    let index = match slot {
        1..=3 => slot - 1,
        5..=9 => slot - 2,
        _ => return None,
    };
    char::from_u32('A' as u32 + u32::try_from(index).ok()?)
}

fn mood_name(mood: Mood) -> &'static str {
    match mood {
        Mood::High => "high",
        Mood::Mid => "mid",
        Mood::Low => "low",
    }
}

fn to_json(surveyed: &SurveyedTrack) -> serde_json::Value {
    let track = &surveyed.track;
    json!({
        "id": track.id,
        "title": track.title,
        "artist": track.artist,
        "genre": track.genre,
        "bpm": track.bpm_x100 as f64 / 100.0,
        "lengthSeconds": track.length,
        "grid": surveyed.grid.as_ref().map(|grid| json!({ "times": grid.times, "beats": grid.beats })),
        "structure": surveyed.structure.as_ref().map(|structure| json!({
            "mood": mood_name(structure.mood),
            "endBeat": structure.end_beat,
            "phrases": structure.phrases.iter().map(|phrase| json!({
                "firstBeat": phrase.first_beat,
                "label": phrase.to_string(),
                "fillInBeat": phrase.fill_in_beat,
            })).collect::<Vec<_>>(),
        })),
        "cues": surveyed.cues.iter().map(|cue| json!({
            "hotSlot": hot_cue_letter(cue.slot).filter(|_| cue.kind == "hot").map(String::from),
            "startMs": cue.start,
            "endMs": cue.end,
        })).collect::<Vec<_>>(),
    })
}

fn nearest_beat(grid: &Grid, ms: i64) -> usize {
    let after = grid.times.partition_point(|&time| i64::from(time) < ms);
    let distance = |index: usize| (i64::from(grid.times[index]) - ms).abs();
    match after {
        0 => 0,
        index if index == grid.times.len() => index - 1,
        index if distance(index - 1) <= distance(index) => index - 1,
        index => index,
    }
}

/// Each phrase named by its kind and how many of that kind came before it, so "Chorus#2" is the
/// second chorus whatever rekordbox's variant number says.
fn phrase_boundaries(structure: &SongStructure) -> Vec<(Beats, String)> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    structure
        .phrases
        .iter()
        .map(|phrase| {
            let kind = phrase.kind.map_or_else(|| "?".to_owned(), |kind| format!("{kind:?}"));
            let occurrence = seen.entry(kind.clone()).and_modify(|n| *n += 1).or_insert(1);
            (Beats::from(phrase.first_beat) - 1, format!("{kind}#{occurrence}"))
        })
        .collect()
}

fn nearest_anchor(boundaries: &[(Beats, String)], beat: Beats) -> Option<PhraseAnchor> {
    boundaries
        .iter()
        .min_by_key(|(start, _)| ((beat - start).abs(), beat > *start))
        .map(|(start, name)| PhraseAnchor { name: name.clone(), beats_after: beat - start })
}

fn placements(surveyed: &SurveyedTrack) -> Vec<Placement> {
    let Some(grid) = &surveyed.grid else { return Vec::new() };
    let first_downbeat = grid.beats.iter().position(|&beat| beat == 1).unwrap_or(0);
    let boundaries = surveyed.structure.as_ref().map(phrase_boundaries).unwrap_or_default();
    let length_ms = (surveyed.track.length * 1000).max(1);
    surveyed
        .cues
        .iter()
        .map(|cue| {
            let beat = nearest_beat(grid, cue.start);
            let on_beat = (i64::from(grid.times[beat]) - cue.start).abs() <= ON_BEAT_TOLERANCE_MS;
            let beat_number = Beats::try_from(beat).unwrap_or(Beats::MAX);
            Placement {
                hot_slot: hot_cue_letter(cue.slot).filter(|_| cue.kind == "hot"),
                beat,
                on_beat,
                on_downbeat: on_beat && grid.beats[beat] == 1,
                beats_from_first_downbeat: beat_number - Beats::try_from(first_downbeat).unwrap_or(0),
                track_fraction: cue.start as f64 / length_ms as f64,
                anchor: nearest_anchor(&boundaries, beat_number),
            }
        })
        .collect()
}

fn percent(count: usize, total: usize) -> String {
    if total == 0 {
        return "  -  ".to_owned();
    }
    format!("{:>4.0}%", 100.0 * count as f64 / total as f64)
}

fn describe_offset(beats: Beats) -> String {
    let (bars, beats) = (beats.div_euclid(BEATS_PER_BAR), beats.rem_euclid(BEATS_PER_BAR));
    match (bars, beats) {
        (0, 0) => "start".to_owned(),
        (bars, 0) => format!("{bars:+} bars"),
        (bars, beats) => format!("{bars:+} bars {beats:+} beats"),
    }
}

fn tally<K: Ord>(items: impl IntoIterator<Item = K>) -> Vec<(K, usize)> {
    let mut counts: BTreeMap<K, usize> = BTreeMap::new();
    for item in items {
        *counts.entry(item).or_default() += 1;
    }
    let mut sorted: Vec<_> = counts.into_iter().collect();
    sorted.sort_by_key(|(_, count)| Reverse(*count));
    sorted
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values.get(values.len() / 2).copied().unwrap_or(f64::NAN)
}

fn report(tracks: &[SurveyedTrack]) {
    let with_grid = tracks.iter().filter(|t| t.grid.is_some()).count();
    let with_structure: Vec<&SongStructure> = tracks.iter().filter_map(|t| t.structure.as_ref()).collect();
    let bpms: Vec<f64> = tracks.iter().map(|t| t.track.bpm_x100 as f64 / 100.0).filter(|bpm| *bpm > 0.0).collect();
    println!("\n== Library ==");
    println!("tracks {}  with grid {with_grid}  with phrases {}", tracks.len(), with_structure.len());
    println!(
        "bpm {:.0}-{:.0} (median {:.0})",
        bpms.iter().copied().fold(f64::INFINITY, f64::min),
        bpms.iter().copied().fold(0.0, f64::max),
        median(bpms.clone())
    );
    println!("moods {:?}", tally(with_structure.iter().map(|s| mood_name(s.mood))));
    println!(
        "hot cues per track {:?}",
        tally(tracks.iter().map(|t| t.cues.iter().filter(|c| c.kind == "hot").count()))
    );
    println!(
        "memory cues per track {:?}",
        tally(tracks.iter().map(|t| t.cues.iter().filter(|c| c.kind == "memory").count()))
    );
    println!("loops {}", tracks.iter().flat_map(|t| &t.cues).filter(|c| c.end.is_some()).count());

    let phrase_bars = with_structure.iter().flat_map(|structure| {
        let starts: Vec<Beats> = structure.phrases.iter().map(|p| Beats::from(p.first_beat)).collect();
        let ends = starts.iter().skip(1).copied().chain([Beats::from(structure.end_beat)]);
        starts.iter().zip(ends).map(|(start, end)| (end - start) as f64 / BEATS_PER_BAR as f64).collect::<Vec<_>>()
    });
    let phrase_bars: Vec<String> = phrase_bars.map(|bars| format!("{bars}")).collect();
    println!("rekordbox phrase lengths in bars {:?}", tally(phrase_bars));

    let placed: Vec<(&SurveyedTrack, Vec<Placement>)> = tracks.iter().map(|t| (t, placements(t))).collect();
    report_alignment(&placed);
    report_phase(&placed);
    report_coverage(&placed);
    report_slots(&placed);
    report_memory(&placed);
    report_gaps(&placed);
}

fn report_alignment(placed: &[(&SurveyedTrack, Vec<Placement>)]) {
    println!("\n== Alignment (share of cues) ==");
    println!(
        "{:<8} {:>5} {:>6} {:>9} {:>5} {:>5} {:>5} {:>5} {:>9} {:>11}",
        "", "cues", "beat", "downbeat", "4bar", "8bar", "16bar", "32bar", "phrase", "phrase±1bar"
    );
    for (name, hot) in [("hot", true), ("memory", false)] {
        let cues: Vec<(&Placement, bool)> = placed
            .iter()
            .flat_map(|(track, places)| places.iter().map(move |p| (p, track.structure.is_some())))
            .filter(|(p, _)| p.hot_slot.is_some() == hot)
            .collect();
        let on_grid = |bars: Bars| {
            cues.iter()
                .filter(|(p, _)| p.on_downbeat && p.beats_from_first_downbeat.rem_euclid(bars * BEATS_PER_BAR) == 0)
                .count()
        };
        let phrased: Vec<&Placement> = cues.iter().filter(|(_, has)| *has).map(|(p, _)| *p).collect();
        let near_phrase = |within: Beats| {
            phrased
                .iter()
                .filter(|p| p.on_beat && p.anchor.as_ref().is_some_and(|a| a.beats_after.abs() <= within))
                .count()
        };
        println!(
            "{name:<8} {:>5} {:>6} {:>9} {:>5} {:>5} {:>5} {:>5} {:>9} {:>11}",
            cues.len(),
            percent(cues.iter().filter(|(p, _)| p.on_beat).count(), cues.len()),
            percent(cues.iter().filter(|(p, _)| p.on_downbeat).count(), cues.len()),
            percent(on_grid(PHRASE_GRIDS[0]), cues.len()),
            percent(on_grid(PHRASE_GRIDS[1]), cues.len()),
            percent(on_grid(PHRASE_GRIDS[2]), cues.len()),
            percent(on_grid(PHRASE_GRIDS[3]), cues.len()),
            percent(near_phrase(0), phrased.len()),
            percent(near_phrase(BEATS_PER_BAR), phrased.len()),
        );
    }
    println!("(N-bar columns count bars from each track's first downbeat; phrase columns only cover tracks with rekordbox phrases)");
}

/// Whether counting bars from the first downbeat finds the phrase grid, or each track needs its
/// own phrase start: the best phase is an upper bound no single rule can beat.
fn report_phase(placed: &[(&SurveyedTrack, Vec<Placement>)]) {
    println!("\n== Phrase grid phase (downbeat cues on the grid) ==");
    let downbeat_cues: Vec<Vec<Beats>> = placed
        .iter()
        .map(|(_, places)| {
            places.iter().filter(|p| p.on_downbeat).map(|p| p.beats_from_first_downbeat / BEATS_PER_BAR).collect()
        })
        .collect();
    let total: usize = downbeat_cues.iter().map(Vec::len).sum();
    for bars in PHRASE_GRIDS {
        let from_first_downbeat: usize =
            downbeat_cues.iter().map(|cues| cues.iter().filter(|b| b.rem_euclid(bars) == 0).count()).sum();
        let best_phase: usize = downbeat_cues
            .iter()
            .map(|cues| {
                (0..bars)
                    .map(|phase| cues.iter().filter(|b| (*b - phase).rem_euclid(bars) == 0).count())
                    .max()
                    .unwrap_or(0)
            })
            .sum();
        println!(
            "{bars:>2} bars: from first downbeat {}  best phase per track {}",
            percent(from_first_downbeat, total),
            percent(best_phase, total)
        );
    }
}

/// Which phrase starts the user cues, out of every one rekordbox found: the choice a cue placer
/// has to learn once it knows the sections.
fn report_coverage(placed: &[(&SurveyedTrack, Vec<Placement>)]) {
    println!("\n== Rekordbox phrase starts that carry a cue ==");
    let mut by_anchor: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut by_kind: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut by_change: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for (track, places) in placed {
        let Some(structure) = &track.structure else { continue };
        let cued: HashSet<Beats> =
            places.iter().filter(|p| p.on_beat).filter_map(|p| Beats::try_from(p.beat).ok()).collect();
        let mut previous_kind = String::new();
        for (start, name) in phrase_boundaries(structure) {
            let carries_cue = usize::from(cued.contains(&start));
            let kind = name.split('#').next().unwrap_or_default().to_owned();
            let change = if kind == previous_kind { "same kind continues" } else { "kind changes" }.to_owned();
            previous_kind.clone_from(&kind);
            for (key, totals) in [(name, &mut by_anchor), (kind, &mut by_kind), (change, &mut by_change)] {
                let (seen, cued) = totals.entry(key).or_default();
                *seen += 1;
                *cued += carries_cue;
            }
        }
    }
    for (title, totals, limit) in [
        ("by change", by_change, usize::MAX),
        ("by kind", by_kind, usize::MAX),
        ("by occurrence", by_anchor, TOP_BOUNDARIES),
    ] {
        println!("  {title}:");
        let mut rows: Vec<_> = totals.into_iter().collect();
        rows.sort_by_key(|(_, (seen, _))| Reverse(*seen));
        for (name, (seen, cued)) in rows.into_iter().take(limit) {
            println!("     {name:<10} {seen:>4} found  {} cued", percent(cued, seen));
        }
    }
}

fn report_slots(placed: &[(&SurveyedTrack, Vec<Placement>)]) {
    println!("\n== Hot cue slots: nearest rekordbox phrase ==");
    let mut by_slot: BTreeMap<char, Vec<&Placement>> = BTreeMap::new();
    for place in placed.iter().flat_map(|(_, places)| places) {
        if let Some(slot) = place.hot_slot {
            by_slot.entry(slot).or_default().push(place);
        }
    }
    for (slot, places) in by_slot {
        let position = median(places.iter().map(|p| p.track_fraction).collect());
        println!("{slot}: {} cues, median at {:.0}% of the track", places.len(), position * 100.0);
        print_anchors(&places);
    }
}

fn print_anchors(places: &[&Placement]) {
    let anchors = places
        .iter()
        .filter_map(|p| p.anchor.as_ref())
        .map(|a| format!("{} {}", a.name, describe_offset(a.beats_after)));
    for (anchor, count) in tally(anchors).into_iter().take(TOP_ANCHORS) {
        println!("     {count:>4}  {anchor}");
    }
}

fn report_memory(placed: &[(&SurveyedTrack, Vec<Placement>)]) {
    println!("\n== Memory cues ==");
    let memory: Vec<&Placement> =
        placed.iter().flat_map(|(_, places)| places).filter(|p| p.hot_slot.is_none()).collect();
    print_anchors(&memory);
    if placed.iter().flat_map(|(_, places)| places).all(|p| p.hot_slot.is_none()) {
        return;
    }
    let to_next_hot = placed.iter().flat_map(|(_, places)| {
        let hot: Vec<usize> = places.iter().filter(|p| p.hot_slot.is_some()).map(|p| p.beat).collect();
        places.iter().filter(|p| p.hot_slot.is_none()).map(move |memory| {
            hot.iter().filter(|&&beat| beat >= memory.beat).min().map_or_else(
                || "no hot cue after".to_owned(),
                |&beat| {
                    format!("{} before a hot cue", describe_offset(Beats::try_from(beat - memory.beat).unwrap_or(0)))
                },
            )
        })
    });
    println!("  distance to the next hot cue:");
    for (distance, count) in tally(to_next_hot).into_iter().take(TOP_ANCHORS) {
        println!("     {count:>4}  {distance}");
    }
}

fn report_gaps(placed: &[(&SurveyedTrack, Vec<Placement>)]) {
    println!("\n== Gaps between neighbouring cues, in bars ==");
    let gaps = placed.iter().flat_map(|(_, places)| {
        let mut beats: Vec<usize> = places.iter().map(|p| p.beat).collect();
        beats.sort_unstable();
        beats.dedup();
        beats
            .windows(2)
            .map(|pair| describe_offset(Beats::try_from(pair[1] - pair[0]).unwrap_or(0)))
            .collect::<Vec<_>>()
    });
    for (gap, count) in tally(gaps).into_iter().take(10) {
        println!("     {count:>4}  {gap}");
    }
}
