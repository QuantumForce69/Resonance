use std::collections::BTreeMap;

use super::AppCore;
use crate::{
    backend::AudioBackend,
    error::{AudioError, Result},
    headset::part::HeadsetPart,
    model::{Command, DeviceState, Flow, Role},
    profile::{next_id, AppOutput, AppSnapshot, PartChoice, ProcessingPart, Profile, ProfileParts},
    settings::AppPrefs,
};

fn not_found() -> AudioError {
    AudioError::NotFound("That profile no longer exists".into())
}

fn clean_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AudioError::Invalid("Give the profile a name".into()));
    }
    Ok(name.to_string())
}

impl<B: AudioBackend> AppCore<B> {
    /// The current setup, for the chosen parts.
    fn capture(&self, choice: PartChoice, headset: Option<HeadsetPart>) -> ProfileParts {
        let apps = choice.apps.then(|| {
            let mut apps: BTreeMap<String, AppSnapshot> = self
                .settings
                .apps
                .iter()
                .map(|(key, p)| (key.clone(), AppSnapshot { volume: p.volume, muted: p.muted, output: AppOutput::Unknown }))
                .collect();
            for a in self.state.apps.iter().filter(|a| !a.key.starts_with("pid:")) {
                let output = match (&a.output, a.is_system) {
                    (_, true) => AppOutput::Unknown,
                    (Some(id), false) => AppOutput::Device(id.clone()),
                    (None, false) => AppOutput::Default,
                };
                apps.insert(a.key.clone(), AppSnapshot { volume: a.volume, muted: a.muted, output });
            }
            apps
        });
        let processing = choice.processing.then(|| ProcessingPart {
            settings: self.settings.processing.clone(),
            call_sends: self.settings.call_sends.clone(),
        });
        let default_output = if choice.default_output {
            self.state
                .devices
                .iter()
                .find(|d| d.flow == Flow::Render && d.default_console && d.state == DeviceState::Active)
                .map(|d| d.id.clone())
        } else {
            None
        };
        ProfileParts { apps, processing, headphones: if choice.headphones { headset } else { None }, default_output }
    }

    fn publish_profiles(&mut self) {
        self.settings_dirty = true;
        self.state.profiles = self.settings.profiles.clone();
        self.state.active_profile = self.settings.active_profile.clone();
    }

    fn profile_index(&self, id: &str) -> Result<usize> {
        self.settings.profiles.iter().position(|p| p.id == id).ok_or_else(not_found)
    }

    pub(super) fn save_profile(&mut self, name: &str, choice: PartChoice, headset: Option<HeadsetPart>) -> Result<()> {
        let name = clean_name(name)?;
        if !choice.any() {
            return Err(AudioError::Invalid("Choose at least one part for the profile".into()));
        }
        if choice.headphones && headset.is_none() {
            return Err(AudioError::Unavailable("Connect the Space One to save its settings".into()));
        }
        let parts = self.capture(choice, headset);
        let id = next_id(&self.settings.profiles);
        self.settings.profiles.push(Profile { id: id.clone(), name, parts });
        self.settings.active_profile = Some(id);
        self.publish_profiles();
        Ok(())
    }

    /// Re-captures the profile's parts; its saved headphone settings stay if the Space One isn't connected.
    pub(super) fn update_profile(&mut self, id: &str, headset: Option<HeadsetPart>) -> Result<()> {
        let i = self.profile_index(id)?;
        let old = self.settings.profiles[i].parts.clone();
        let mut parts = self.capture(old.choice(), headset);
        if old.headphones.is_some() && parts.headphones.is_none() {
            parts.headphones = old.headphones;
        }
        self.settings.profiles[i].parts = parts;
        self.publish_profiles();
        Ok(())
    }

    pub(super) fn rename_profile(&mut self, id: &str, name: &str) -> Result<()> {
        let name = clean_name(name)?;
        let i = self.profile_index(id)?;
        self.settings.profiles[i].name = name;
        self.publish_profiles();
        Ok(())
    }

    pub(super) fn delete_profile(&mut self, id: &str) -> Result<()> {
        let i = self.profile_index(id)?;
        self.settings.profiles.remove(i);
        if self.settings.active_profile.as_deref() == Some(id) {
            self.settings.active_profile = None;
        }
        self.publish_profiles();
        Ok(())
    }

    /// Applies every part the profile has; problems become notices and never stop the other parts.
    pub(super) fn apply_profile(&mut self, id: &str) -> Result<()> {
        let parts = self.settings.profiles[self.profile_index(id)?].parts.clone();
        if let Some(apps) = &parts.apps {
            self.apply_apps(apps);
        }
        if let Some(processing) = parts.processing {
            self.settings.call_sends = processing.call_sends;
            self.settings_dirty = true;
            if let Err(e) = self.apply(Command::SetProcessing { processing: processing.settings }) {
                self.notices.push(e);
            }
        }
        if let Some(output) = &parts.default_output {
            self.apply_default_output(output);
        }
        self.settings.active_profile = Some(id.to_string());
        self.settings_dirty = true;
        self.refresh()?;
        Ok(())
    }

    fn apply_apps(&mut self, apps: &BTreeMap<String, AppSnapshot>) {
        for (key, snap) in apps {
            self.settings.apps.insert(key.clone(), AppPrefs { volume: snap.volume, muted: snap.muted });
            let running = self.state.apps.iter().find(|a| &a.key == key).map(|a| (a.name.clone(), a.session_ids.clone(), a.pids.clone()));
            match running {
                Some((name, sessions, pids)) => {
                    for sid in &sessions {
                        let _ = self.backend.set_session_volume(sid, snap.volume);
                        let _ = self.backend.set_session_mute(sid, snap.muted);
                    }
                    self.route_app(&name, &pids, &snap.output);
                }
                None if snap.output != AppOutput::Unknown => {
                    self.pending_outputs.insert(key.clone(), snap.output.clone());
                }
                None => {}
            }
        }
        self.settings_dirty = true;
    }

    /// Sends an app's processes to its saved output; a saved device that's unplugged becomes a notice.
    pub(super) fn route_app(&mut self, name: &str, pids: &[u32], output: &AppOutput) {
        if !self.backend.routing_available() {
            return;
        }
        let target = match output {
            AppOutput::Unknown => return,
            AppOutput::Default => None,
            AppOutput::Device(id) => {
                if !self.state.devices.iter().any(|d| &d.id == id && d.state == DeviceState::Active) {
                    self.notices.push(AudioError::NotFound(format!("{name}'s saved output isn't connected")));
                    return;
                }
                Some(id.as_str())
            }
        };
        for &pid in pids {
            let _ = self.backend.set_app_output(pid, target);
        }
    }

    fn apply_default_output(&mut self, id: &str) {
        let found = self.state.devices.iter().find(|d| d.id == id && d.flow == Flow::Render).map(|d| (d.state == DeviceState::Active, d.name.clone()));
        match found {
            Some((true, _)) => {
                for role in [Role::Console, Role::Communications] {
                    if let Err(e) = self.backend.set_default_device(id, role) {
                        self.notices.push(e);
                        break;
                    }
                }
            }
            Some((false, name)) => {
                self.notices.push(AudioError::NotFound(format!("{name} isn't connected, so the default output wasn't changed")));
            }
            None => self.notices.push(AudioError::NotFound(
                "The profile's output device isn't connected, so the default output wasn't changed".into(),
            )),
        }
    }
}
