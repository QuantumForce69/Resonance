use std::collections::BTreeMap;

use crate::model::{App, RawSession};

pub const SYSTEM_KEY: &str = "#system";

fn file_name(path: &str) -> Option<&str> {
    path.rsplit(['\\', '/']).next().filter(|s| !s.is_empty())
}

/// Stable per-app key: lowercase exe file name, `#system`, or `pid:<n>` when the exe is unknown.
pub fn app_key(s: &RawSession) -> String {
    if s.is_system {
        return SYSTEM_KEY.into();
    }
    match s.exe_path.as_deref().and_then(file_name) {
        Some(f) => f.to_lowercase(),
        None => format!("pid:{}", s.pid),
    }
}

fn default_name(a: &App) -> String {
    if a.is_system {
        return "System sounds".into();
    }
    if let Some(f) = a.exe_path.as_deref().and_then(file_name) {
        let stem = f.rsplit_once('.').map(|(s, _)| s).unwrap_or(f);
        let mut chars = stem.chars();
        return match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => f.to_string(),
        };
    }
    format!("Process {}", a.pids.first().copied().unwrap_or(0))
}

/// Groups sessions by app key. Sorted by name, system sounds last.
pub fn group_sessions(sessions: &[RawSession]) -> Vec<App> {
    let mut map: BTreeMap<String, App> = BTreeMap::new();
    for s in sessions {
        let key = app_key(s);
        let app = map.entry(key.clone()).or_insert_with(|| App {
            key,
            name: String::new(),
            exe_path: s.exe_path.clone(),
            is_system: s.is_system,
            pids: Vec::new(),
            session_ids: Vec::new(),
            volume: s.volume,
            muted: s.muted,
            output: None,
        });
        if app.name.is_empty() {
            if let Some(n) = s.display_name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
                app.name = n.to_string();
            }
        }
        if s.pid != 0 && !app.pids.contains(&s.pid) {
            app.pids.push(s.pid);
        }
        app.session_ids.push(s.id.clone());
        if app.output.is_none() {
            app.output = s.output.clone();
        }
    }
    let mut apps: Vec<App> = map
        .into_values()
        .map(|mut a| {
            if a.name.is_empty() {
                a.name = default_name(&a);
            }
            a
        })
        .collect();
    apps.sort_by(|a, b| {
        a.is_system.cmp(&b.is_system).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    apps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeBackend;

    #[test]
    fn keys_by_lowercase_exe_name_system_and_pid() {
        let mut sys = FakeBackend::session("s", 0, None);
        sys.is_system = true;
        assert_eq!(app_key(&sys), SYSTEM_KEY);
        assert_eq!(app_key(&FakeBackend::session("a", 1, Some("Spotify.EXE"))), "spotify.exe");
        assert_eq!(app_key(&FakeBackend::session("b", 42, None)), "pid:42");
    }

    #[test]
    fn groups_sessions_of_the_same_exe() {
        let mut a1 = FakeBackend::session("a1", 10, Some("chrome.exe"));
        a1.volume = 0.3;
        let a2 = FakeBackend::session("a2", 11, Some("chrome.exe"));
        let b = FakeBackend::session("b", 20, Some("discord.exe"));
        let apps = group_sessions(&[a1, a2, b]);
        assert_eq!(apps.len(), 2);
        let chrome = apps.iter().find(|a| a.key == "chrome.exe").unwrap();
        assert_eq!(chrome.session_ids, vec!["a1", "a2"]);
        assert_eq!(chrome.pids, vec![10, 11]);
        assert_eq!(chrome.volume, 0.3, "group volume comes from the first session");
    }

    #[test]
    fn names_prefer_display_name_then_exe_stem() {
        let mut a = FakeBackend::session("a", 1, Some("spotify.exe"));
        a.display_name = Some("Spotify Music".into());
        let b = FakeBackend::session("b", 2, Some("obs64.exe"));
        let mut sys = FakeBackend::session("s", 0, None);
        sys.is_system = true;
        let c = FakeBackend::session("c", 77, None);
        let apps = group_sessions(&[sys, a, b, c]);
        let names: Vec<_> = apps.iter().map(|a| a.name.as_str()).collect();
        // sorted alphabetically, system sounds last
        assert_eq!(names, vec!["Obs64", "Process 77", "Spotify Music", "System sounds"]);
    }

    #[test]
    fn output_comes_from_any_session_that_has_one() {
        let a1 = FakeBackend::session("a1", 10, Some("game.exe"));
        let mut a2 = FakeBackend::session("a2", 10, Some("game.exe"));
        a2.output = Some("out2".into());
        let apps = group_sessions(&[a1, a2]);
        assert_eq!(apps[0].output.as_deref(), Some("out2"));
        assert_eq!(apps[0].pids, vec![10]);
    }
}
