use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{engine::ProcessingSettings, headset::part::HeadsetPart};

/// Where an app's sound goes, as saved in a profile.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "camelCase")]
pub enum AppOutput {
    /// The app wasn't playing when the profile was saved, so its route isn't known: left alone.
    #[default]
    Unknown,
    /// Follows the Windows default output.
    Default,
    Device(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub volume: f32,
    pub muted: bool,
    #[serde(default)]
    pub output: AppOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingPart {
    pub settings: ProcessingSettings,
    pub call_sends: BTreeMap<String, f32>,
}

/// The parts a profile controls; absent parts are left alone when it's applied.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProfileParts {
    pub apps: Option<BTreeMap<String, AppSnapshot>>,
    pub processing: Option<ProcessingPart>,
    pub headphones: Option<HeadsetPart>,
    pub default_output: Option<String>,
}

/// Which parts to capture when saving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PartChoice {
    pub apps: bool,
    pub processing: bool,
    pub headphones: bool,
    pub default_output: bool,
}

impl PartChoice {
    pub fn any(&self) -> bool {
        self.apps || self.processing || self.headphones || self.default_output
    }
}

impl ProfileParts {
    /// The parts this profile has, for re-capturing it.
    pub fn choice(&self) -> PartChoice {
        PartChoice {
            apps: self.apps.is_some(),
            processing: self.processing.is_some(),
            headphones: self.headphones.is_some(),
            default_output: self.default_output.is_some(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub parts: ProfileParts,
}

/// An id no existing profile uses: `p1`, `p2`, …
pub fn next_id(profiles: &[Profile]) -> String {
    let highest = profiles.iter().filter_map(|p| p.id.strip_prefix('p')?.parse::<u32>().ok()).max().unwrap_or(0);
    format!("p{}", highest + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str) -> Profile {
        Profile { id: id.into(), name: id.into(), parts: ProfileParts::default() }
    }

    #[test]
    fn ids_count_up_past_the_highest() {
        assert_eq!(next_id(&[]), "p1");
        assert_eq!(next_id(&[profile("p1"), profile("p7"), profile("odd")]), "p8");
    }

    #[test]
    fn choice_mirrors_the_parts() {
        let parts = ProfileParts { default_output: Some("out1".into()), ..Default::default() };
        assert_eq!(parts.choice(), PartChoice { default_output: true, ..Default::default() });
        assert!(parts.choice().any());
        assert!(!PartChoice::default().any());
    }

    #[test]
    fn serializes_camel_case_with_tagged_outputs() {
        let mut apps = BTreeMap::new();
        apps.insert("spotify.exe".to_string(), AppSnapshot { volume: 0.5, muted: false, output: AppOutput::Device("out2".into()) });
        apps.insert("game.exe".to_string(), AppSnapshot { volume: 1.0, muted: true, output: AppOutput::Unknown });
        let p = Profile { id: "p1".into(), name: "Gaming".into(), parts: ProfileParts { apps: Some(apps), default_output: Some("out1".into()), ..Default::default() } };
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["parts"]["apps"]["spotify.exe"]["output"]["kind"], "device");
        assert_eq!(v["parts"]["apps"]["spotify.exe"]["output"]["id"], "out2");
        assert_eq!(v["parts"]["apps"]["game.exe"]["output"]["kind"], "unknown");
        assert_eq!(v["parts"]["defaultOutput"], "out1");
        assert!(v["parts"]["headphones"].is_null());
        let back: Profile = serde_json::from_value(v).unwrap();
        assert_eq!(back, p);
    }
}
