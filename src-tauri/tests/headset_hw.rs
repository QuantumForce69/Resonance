use resonance_lib::headset::{
    device::HeadsetConnector,
    mapping::{command_values, read_state},
    model::{AmbientMode, HeadsetCommand},
    LibConnector,
};

/// Needs the real Space One switched on and connected to this PC.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn real_space_one_reads_and_restores_ambient_mode() {
    let dir = tempfile::tempdir().unwrap();
    let connector = LibConnector::open(dir.path().join("openscq30.sqlite"), false).await.expect("session");
    let found = connector.connect().await.expect("connect").expect("Space One not found — is it on and paired?");
    let before = read_state(found.device.as_ref(), &found.name);
    println!("{before:#?}");
    assert!(before.battery.is_some(), "battery level should be reported");
    let original = before.ambient_mode.expect("ambient mode");
    let other = if original == AmbientMode::Normal { AmbientMode::Transparency } else { AmbientMode::Normal };

    found.device.set_setting_values(command_values(&HeadsetCommand::SetAmbientMode { mode: other }, &before).unwrap()).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert_eq!(read_state(found.device.as_ref(), &found.name).ambient_mode, Some(other));

    found.device.set_setting_values(command_values(&HeadsetCommand::SetAmbientMode { mode: original }, &before).unwrap()).await.unwrap();
}
