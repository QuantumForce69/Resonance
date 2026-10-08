//! Measures the processing chain's cost per second of audio. Run (optimised, separate target dir) with:
//! CARGO_TARGET_DIR=src-tauri/target-bench cargo test --release --manifest-path src-tauri/Cargo.toml --test bench_dsp -- --ignored --nocapture
use std::time::Instant;

use resonance_lib::engine::{
    dsp::{callbus::CallBus, chain::Chain, FRAME},
    source::SourceQueue,
    CallSettings, DspParams,
};

fn signal(seconds: usize) -> Vec<f32> {
    let mut seed = 1u32;
    (0..seconds * 48_000)
        .map(|i| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = ((seed >> 8) as f32 / 16_777_216.0 - 0.5) * 0.01;
            let voice = if (i / 4800) % 2 == 0 { 0.3 * (i as f32 * 0.03).sin() } else { 0.0 };
            noise + voice
        })
        .collect()
}

fn time(label: &str, seconds: usize, mut f: impl FnMut(&mut [f32; FRAME])) {
    let mut audio = signal(seconds);
    let t = Instant::now();
    for frame in audio.as_chunks_mut::<FRAME>().0 {
        f(frame);
    }
    let per_second = t.elapsed() / seconds as u32;
    println!("BENCH {label:<28} {per_second:>10.2?} per second of audio = {:.3}% of a core", per_second.as_secs_f64() * 100.0);
}

#[test]
#[ignore]
fn bench_chain() {
    let all = DspParams::default();
    let mut chain = Chain::new(&all);
    time("full chain", 10, |f| {
        chain.process(f);
    });
    let mut no_denoise = all;
    no_denoise.denoise.enabled = false;
    let mut chain = Chain::new(&no_denoise);
    time("gate+comp+limiter", 10, |f| {
        chain.process(f);
    });
    for (name, stage) in [("denoise only", 0), ("gate only", 1), ("compressor only", 2), ("limiter only", 3)] {
        let mut p = all;
        p.denoise.enabled = stage == 0;
        p.gate.enabled = stage == 1;
        p.compressor.enabled = stage == 2;
        p.limiter.enabled = stage == 3;
        let mut chain = Chain::new(&p);
        time(name, 10, |f| {
            chain.process(f);
        });
    }
    let mut bus = CallBus::new();
    let mut queue = SourceQueue::new();
    let mut block = [0.0f32; FRAME];
    let call = CallSettings::default();
    time("one sent app (queue+bus)", 10, |f| {
        queue.push(&f[..]);
        queue.pull(&mut block);
        bus.clear();
        bus.add(&block, 0.7);
        bus.finish(f, &call, -45.0);
    });
}
