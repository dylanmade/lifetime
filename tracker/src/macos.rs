use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use core_foundation_sys::base::CFTypeRef;
use core_foundation_sys::dictionary::CFDictionaryRef;
use core_foundation_sys::string::CFStringRef;
use lifetime_core::model::{AppUsageSample, ObservationKind};
use objc2::msg_send;
use objc2::rc::autoreleasepool;
use objc2_app_kit::NSWorkspace;

use crate::sampler::Sampler;

/// Seconds of input inactivity before we consider the user away.
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
    let pid: i32 = unsafe { msg_send![&*app, processIdentifier] };
    let is_active = idle_seconds() < IDLE_THRESHOLD_SECONDS;
    let window_title = focused_window_title(pid);
    Some(ObservationKind::AppUsage(AppUsageSample {
        bundle_id,
        app_name,
        window_title,
        is_active,
    }))
}

/// Returns true if this app has been granted Accessibility permission.
/// Never prompts.
pub fn is_accessibility_granted() -> bool {
    unsafe { AXIsProcessTrustedWithOptions(std::ptr::null()) }
}

/// Returns true if Accessibility permission is already granted. If not,
/// shows the OS-level dialog directing the user to System Settings.
pub fn request_accessibility_permission() -> bool {
    let prompt_key = CFString::from_static_string("AXTrustedCheckOptionPrompt");
    let prompt_value = CFBoolean::true_value();
    let options =
        CFDictionary::from_CFType_pairs(&[(prompt_key.as_CFType(), prompt_value.as_CFType())]);
    unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) }
}

fn focused_window_title(pid: i32) -> Option<String> {
    if !is_accessibility_granted() {
        return None;
    }
    unsafe {
        let app_element = AXUIElementCreateApplication(pid);
        if app_element.is_null() {
            return None;
        }
        let _app_guard = AXElementGuard(app_element);

        let focused_attr = CFString::from_static_string("AXFocusedWindow");
        let mut window_ref: CFTypeRef = std::ptr::null();
        let err = AXUIElementCopyAttributeValue(
            app_element,
            focused_attr.as_concrete_TypeRef(),
            &mut window_ref,
        );
        if err != 0 || window_ref.is_null() {
            return None;
        }
        let _window_guard = AXElementGuard(window_ref as AXUIElementRef);

        let title_attr = CFString::from_static_string("AXTitle");
        let mut title_ref: CFTypeRef = std::ptr::null();
        let err = AXUIElementCopyAttributeValue(
            window_ref as AXUIElementRef,
            title_attr.as_concrete_TypeRef(),
            &mut title_ref,
        );
        if err != 0 || title_ref.is_null() {
            return None;
        }
        let title = CFString::wrap_under_create_rule(title_ref as CFStringRef);
        let s = title.to_string();
        if s.is_empty() { None } else { Some(s) }
    }
}

fn idle_seconds() -> f64 {
    const HID_SYSTEM_STATE: u32 = 1;
    const ANY_INPUT_EVENT_TYPE: u32 = !0u32;
    unsafe { CGEventSourceSecondsSinceLastEventType(HID_SYSTEM_STATE, ANY_INPUT_EVENT_TYPE) }
}

// ---- FFI ----

type AXUIElementRef = *mut std::ffi::c_void;
type AXError = i32;

/// RAII guard that CFRelease's an AXUIElementRef on drop.
struct AXElementGuard(AXUIElementRef);

impl Drop for AXElementGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0 as CFTypeRef) };
        }
    }
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventSourceSecondsSinceLastEventType(source_state_id: u32, event_type: u32) -> f64;
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: CFTypeRef);
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

    #[test]
    fn accessibility_check_does_not_panic() {
        // Just verifies the FFI call returns. The actual permission state
        // depends on the test runner's environment.
        let _ = is_accessibility_granted();
    }
}
