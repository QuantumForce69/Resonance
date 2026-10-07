use resonance_lib::headset::{
    device::HeadsetConnector,
    mapping::{command_values, read_state},
    model::{AmbientMode, Connection, HeadsetCommand},
    LibConnector,
};

#[tokio::test(flavor = "multi_thread")]
async fn simulated_space_one_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let connector = LibConnector::open(dir.path().join("openscq30.sqlite"), true).await.expect("session");
    let found = connector.connect().await.expect("connect").expect("a demo Space One");
    let state = read_state(found.device.as_ref(), &found.name);
    assert_eq!(state.connection, Connection::Connected);
    assert!(state.ambient_mode.is_some());
    let eq = state.eq.clone().expect("eq");
    assert_eq!(eq.band_hz.len(), 8);
    assert!(eq.presets.len() >= 20, "expected the full preset list, got {}", eq.presets.len());

    let values = command_values(&HeadsetCommand::SetAmbientMode { mode: AmbientMode::Transparency }, &state).unwrap();
    found.device.set_setting_values(values).await.expect("write");
    let after = read_state(found.device.as_ref(), &found.name);
    assert_eq!(after.ambient_mode, Some(AmbientMode::Transparency));

    // Reconnecting reuses the stored pairing.
    assert!(connector.connect().await.expect("reconnect").is_some());
}
