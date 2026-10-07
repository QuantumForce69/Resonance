#![cfg(windows)]
use resonance_lib::{backend::AudioBackend, win::WasapiBackend};
use std::time::Instant;

/// Measures one OS poll (snapshot) and one meter read. Run with --ignored --nocapture.
#[test]
#[ignore]
fn bench_snapshot_and_peaks() {
    let mut b = WasapiBackend::new().unwrap();
    b.snapshot().unwrap(); // warm caches
    let n = 40;
    let t = Instant::now();
    for _ in 0..n {
        b.snapshot().unwrap();
    }
    let snap = t.elapsed() / n;
    let t = Instant::now();
    for _ in 0..n {
        b.peaks(None);
    }
    let peaks = t.elapsed() / n;
    let s = b.snapshot().unwrap();
    println!("BENCH snapshot={snap:?} peaks={peaks:?} devices={} sessions={}", s.devices.len(), s.sessions.len());
}
