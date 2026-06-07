use std::{cmp::Reverse, collections::HashSet, fs, path::Path, time::SystemTime};

use serde::Serialize;
use windows::Win32::{
    Foundation::{HWND, LPARAM},
    UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        IsWindowVisible,
    },
};

use super::running_processes::list_running_processes;

pub const POTPLAYER_DEFAULT_PROCESS_NAME: &str = "PotPlayerMini64.exe";
const POTPLAYER_PROCESS_NAMES: &[&str] = &["potplayermini64.exe", "potplayermini.exe"];

#[derive(Debug, Clone, Serialize)]
pub struct PotPlayerMediaInfo {
    pub process_name: String,
    pub media_path: Option<String>,
    pub media_directory: Option<String>,
    pub window_title: String,
    pub source: Option<String>,
}

#[derive(Debug, Clone)]
struct PlaylistSnapshot {
    modified_at: SystemTime,
    playname: Option<String>,
    entries: Vec<String>,
}

#[derive(Debug, Clone)]
struct PotPlayerProcess {
    process_name: String,
    process_path: Option<String>,
    window_title: String,
}

struct WindowSearchState<'a> {
    process_ids: &'a HashSet<u32>,
    titles: &'a mut Vec<String>,
}

pub fn is_supported_potplayer_process(process_name: &str) -> bool {
    POTPLAYER_PROCESS_NAMES.contains(&process_name.to_ascii_lowercase().as_str())
}

pub fn get_current_potplayer_media() -> Result<PotPlayerMediaInfo, String> {
    let process = find_running_potplayer_process()?
        .ok_or_else(|| "未检测到正在运行的 PotPlayer。".to_string())?;

    let fallback = PotPlayerMediaInfo {
        process_name: process.process_name.clone(),
        media_path: None,
        media_directory: None,
        window_title: process.window_title.clone(),
        source: None,
    };

    Ok(detect_potplayer_media_for_process(
        &process.process_name,
        process.process_path.as_deref(),
        Some(&process.window_title),
    )
    .unwrap_or(fallback))
}

pub fn detect_potplayer_media_for_process(
    process_name: &str,
    process_path: Option<&str>,
    preferred_window_title: Option<&str>,
) -> Option<PotPlayerMediaInfo> {
    if !is_supported_potplayer_process(process_name) {
        return None;
    }

    let window_title = preferred_window_title
        .unwrap_or_default()
        .trim()
        .to_string();
    let playlist_dir = process_path
        .and_then(|path| Path::new(path).parent())
        .map(|directory| directory.join("Playlist"));
    let playlists = playlist_dir.as_deref().and_then(read_playlist_snapshots)?;
    let media_path = select_playlist_media_path(&window_title, &playlists)
        .or_else(|| extract_full_path_from_window_title(&window_title));
    let media_directory = media_path
        .as_deref()
        .and_then(|value| Path::new(value).parent())
        .map(|directory| normalize_path(directory.to_string_lossy().as_ref()));
    let source = media_path.as_ref().and_then(|path| {
        if looks_like_full_path_in_title(&window_title)
            && path.eq_ignore_ascii_case(
                window_title_media_segment(&window_title)
                    .as_deref()
                    .unwrap_or_default(),
            )
        {
            Some("window_title".to_string())
        } else if !path.is_empty() {
            Some("playlist".to_string())
        } else {
            None
        }
    });

    Some(PotPlayerMediaInfo {
        process_name: process_name.to_string(),
        media_path,
        media_directory,
        window_title,
        source,
    })
}

fn find_running_potplayer_process() -> Result<Option<PotPlayerProcess>, String> {
    let running_processes = list_running_processes()?;
    let mut potplayer_processes = running_processes
        .into_iter()
        .filter(|process| is_supported_potplayer_process(&process.process_name))
        .collect::<Vec<_>>();

    if potplayer_processes.is_empty() {
        return Ok(None);
    }

    let process_ids = potplayer_processes
        .iter()
        .map(|process| process.process_id)
        .collect::<HashSet<_>>();
    let window_titles = find_window_titles_for_processes(&process_ids);
    potplayer_processes.sort_by(|left, right| left.process_name.cmp(&right.process_name));

    let selected = potplayer_processes
        .into_iter()
        .find_map(|process| {
            let window_title = window_titles
                .iter()
                .find(|(process_id, title)| {
                    *process_id == process.process_id && !title.trim().is_empty()
                })
                .map(|(_, title)| title.clone())?;
            Some(PotPlayerProcess {
                process_name: process.process_name,
                process_path: process.process_path,
                window_title,
            })
        })
        .or_else(|| {
            let mut titles = window_titles.into_iter();
            let (process_id, window_title) = titles.next()?;
            let process = list_running_processes()
                .ok()?
                .into_iter()
                .find(|item| item.process_id == process_id)?;
            Some(PotPlayerProcess {
                process_name: process.process_name,
                process_path: process.process_path,
                window_title,
            })
        });

    Ok(selected.or_else(|| {
        list_running_processes()
            .ok()
            .and_then(|processes| {
                processes
                    .into_iter()
                    .find(|process| is_supported_potplayer_process(&process.process_name))
            })
            .map(|process| PotPlayerProcess {
                process_name: process.process_name,
                process_path: process.process_path,
                window_title: String::new(),
            })
    }))
}

fn find_window_titles_for_processes(process_ids: &HashSet<u32>) -> Vec<(u32, String)> {
    let mut titles = Vec::new();
    let mut state = WindowSearchState {
        process_ids,
        titles: &mut titles,
    };

    unsafe {
        let _ = EnumWindows(
            Some(enum_windows_for_process_titles),
            LPARAM((&mut state as *mut WindowSearchState<'_>) as isize),
        );
    }

    titles
        .into_iter()
        .filter_map(|entry| {
            let (process_id, title) = entry.split_once('\u{0}')?;
            Some((process_id.parse::<u32>().ok()?, title.to_string()))
        })
        .collect()
}

unsafe extern "system" fn enum_windows_for_process_titles(
    window: HWND,
    lparam: LPARAM,
) -> windows::core::BOOL {
    let Some(state) = (unsafe { (lparam.0 as *mut WindowSearchState<'_>).as_mut() }) else {
        return windows::core::BOOL(1);
    };

    if unsafe { !IsWindowVisible(window).as_bool() } {
        return windows::core::BOOL(1);
    }

    let mut process_id = 0;
    unsafe {
        GetWindowThreadProcessId(window, Some(&mut process_id));
    }
    if process_id == 0 || !state.process_ids.contains(&process_id) {
        return windows::core::BOOL(1);
    }

    let title = read_window_title(window);
    if title.trim().is_empty() {
        return windows::core::BOOL(1);
    }

    state.titles.push(format!("{process_id}\u{0}{title}"));
    windows::core::BOOL(1)
}

fn read_window_title(window: HWND) -> String {
    let length = unsafe { GetWindowTextLengthW(window) };
    if length <= 0 {
        return String::new();
    }

    let mut buffer = vec![0u16; length as usize + 1];
    let copied = unsafe { GetWindowTextW(window, &mut buffer) };
    String::from_utf16_lossy(&buffer[..copied as usize])
}

fn read_playlist_snapshots(playlist_dir: &Path) -> Option<Vec<PlaylistSnapshot>> {
    let entries = fs::read_dir(playlist_dir).ok()?;
    let mut snapshots = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            let extension = path.extension()?.to_str()?;
            if !extension.eq_ignore_ascii_case("dpl") {
                return None;
            }

            let modified_at = entry.metadata().ok()?.modified().ok()?;
            let content = fs::read(&path).ok()?;
            let snapshot = parse_playlist_snapshot(&content)?;
            Some(PlaylistSnapshot {
                modified_at,
                playname: snapshot.playname,
                entries: snapshot.entries,
            })
        })
        .collect::<Vec<_>>();

    snapshots.sort_by_key(|snapshot| Reverse(snapshot.modified_at));
    Some(snapshots)
}

fn select_playlist_media_path(
    window_title: &str,
    playlists: &[PlaylistSnapshot],
) -> Option<String> {
    let title_file_name = extract_file_name_from_window_title(window_title);

    if let Some(title_file_name) = title_file_name.as_deref() {
        for playlist in playlists {
            if let Some(playname) = playlist.playname.as_deref() {
                if file_name_matches(playname, title_file_name) {
                    return Some(normalize_path(playname));
                }
            }
            if let Some(entry) = playlist
                .entries
                .iter()
                .find(|entry| file_name_matches(entry, title_file_name))
            {
                return Some(normalize_path(entry));
            }
        }
    }

    playlists
        .iter()
        .find_map(|playlist| playlist.playname.as_deref().map(normalize_path))
        .or_else(|| {
            playlists
                .iter()
                .find_map(|playlist| playlist.entries.first().map(|entry| normalize_path(entry)))
        })
}

fn extract_file_name_from_window_title(window_title: &str) -> Option<String> {
    let media_segment = window_title_media_segment(window_title)?;
    let path = Path::new(&media_segment);
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let trimmed = media_segment.trim();
            (!trimmed.is_empty()).then_some(trimmed.to_string())
        })
}

fn extract_full_path_from_window_title(window_title: &str) -> Option<String> {
    let media_segment = window_title_media_segment(window_title)?;
    looks_like_full_path(&media_segment).then_some(normalize_path(&media_segment))
}

fn window_title_media_segment(window_title: &str) -> Option<String> {
    let trimmed = window_title.trim();
    if trimmed.is_empty() {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();
    let media_segment = if let Some(index) = lower.rfind(" - potplayer") {
        &trimmed[..index]
    } else {
        trimmed
    };

    let segment = media_segment.trim();
    (!segment.is_empty()).then_some(segment.to_string())
}

fn looks_like_full_path_in_title(window_title: &str) -> bool {
    window_title_media_segment(window_title)
        .as_deref()
        .is_some_and(looks_like_full_path)
}

fn looks_like_full_path(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.contains(":\\") || trimmed.starts_with("\\\\")
}

fn file_name_matches(path: &str, file_name: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(file_name))
}

fn normalize_path(value: &str) -> String {
    value
        .trim()
        .trim_matches('"')
        .replace('/', "\\")
        .trim_end_matches(['\\', '/'])
        .to_string()
}

#[derive(Debug, Clone)]
struct ParsedPlaylistSnapshot {
    playname: Option<String>,
    entries: Vec<String>,
}

fn parse_playlist_snapshot(bytes: &[u8]) -> Option<ParsedPlaylistSnapshot> {
    let content = String::from_utf8_lossy(bytes);
    let mut playname = None;
    let mut entries = Vec::new();

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if let Some(value) = line.strip_prefix("playname=") {
            let normalized = normalize_path(value);
            if !normalized.is_empty() {
                playname = Some(normalized);
            }
            continue;
        }

        let Some((_, file_path)) = line.split_once("*file*") else {
            continue;
        };
        let normalized = normalize_path(file_path);
        if !normalized.is_empty() {
            entries.push(normalized);
        }
    }

    (!entries.is_empty() || playname.is_some())
        .then_some(ParsedPlaylistSnapshot { playname, entries })
}

#[cfg(test)]
mod tests {
    use super::{parse_playlist_snapshot, select_playlist_media_path, PlaylistSnapshot};
    use std::time::SystemTime;

    #[test]
    fn parses_playname_from_playlist_file() {
        let snapshot = parse_playlist_snapshot(
            br#"DAUMPLAYLIST
playname=D:\Videos\Season 1\Episode 07.mkv
1*file*D:\Videos\Season 1\Episode 01.mkv
7*file*D:\Videos\Season 1\Episode 07.mkv
"#,
        )
        .expect("snapshot");

        assert_eq!(
            snapshot.playname.as_deref(),
            Some(r"D:\Videos\Season 1\Episode 07.mkv")
        );
        assert_eq!(snapshot.entries.len(), 2);
    }

    #[test]
    fn title_matching_prefers_matching_entry_over_stale_playname() {
        let playlists = vec![PlaylistSnapshot {
            modified_at: SystemTime::now(),
            playname: Some(r"D:\Videos\Season 1\Episode 05.mkv".to_string()),
            entries: vec![
                r"D:\Videos\Season 1\Episode 05.mkv".to_string(),
                r"D:\Videos\Season 1\Episode 07.mkv".to_string(),
            ],
        }];

        let selected = select_playlist_media_path("Episode 07.mkv - PotPlayer", &playlists);

        assert_eq!(
            selected.as_deref(),
            Some(r"D:\Videos\Season 1\Episode 07.mkv")
        );
    }
}
