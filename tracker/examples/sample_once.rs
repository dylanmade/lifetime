//! Smoke test: construct the platform sampler, take one sample, print it.
//!
//! Run with:
//!   cargo run -p lifetime-tracker --example sample_once

#[cfg(target_os = "macos")]
fn main() {
    use lifetime_tracker::Sampler;
    use lifetime_tracker::macos::MacOsSampler;

    let sampler = MacOsSampler::new();
    let samples = sampler.sample();
    println!("{samples:#?}");
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("No sampler implementation for this target_os yet.");
}
