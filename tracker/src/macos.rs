use lifetime_core::model::{AppUsageSample, ObservationKind};
use objc2::rc::autoreleasepool;
use objc2_app_kit::NSWorkspace;

use crate::sampler::Sampler;

/// Seconds of input inactivity before we consider the user away. Anything
/// shorter would flap during natural reading/thinking pauses.
const IDLE_THRESHOLD_SECONDS: f64 = 60.0;

pub struct MacOsSampler;

impl MacOsSampler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MacOsSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Sampler for MacOsSampler {
    fn sample(&self) -> Vec<ObservationKind> {
        autoreleasepool(|_| sample_app().into_iter().collect())
    }
}

fn sample_app() -> Option<ObservationKind> {
    let workspace = unsafe { NSWorkspace::sharedWorkspace() };
    let app = unsafe { workspace.frontmostApplication() }?;
    let bundle_id = unsafe { app.bundleIdentifier() }.map(|s| s.to_string());
    let app_name = unsafe { app.localizedName() }.map(|s| s.to_string())?;
    let is_active = idle_seconds() < IDLE_THRESHOLD_SECONDS;
    Some(ObservationKind::AppUsage(AppUsageSample {
        bundle_id,
        app_name,
        window_title: None,
        is_active,
    }))
}

/// Seconds since the last HID input event (any key/mouse/trackpad).
fn idle_seconds() -> f64 {
    // CGEventSourceStateID::HIDSystemState = 1
    // kCGAnyInputEventType = ~0u32
    const HID_SYSTEM_STATE: u32 = 1;
    const ANY_INPUT_EVENT_TYPE: u32 = !0u32;
    unsafe { CGEventSourceSecondsSinceLastEventType(HID_SYSTEM_STATE, ANY_INPUT_EVENT_TYPE) }
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventSourceSecondsSinceLastEventType(
        source_state_id: u32,
        event_type: u32,
    ) -> f64;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampler_constructs() {
        let _ = MacOsSampler::new();
    }

    #[test]
    fn idle_seconds_returns_finite_non_negative() {
        let s = idle_seconds();
        assert!(s.is_finite());
        assert!(s >= 0.0);
    }
}
