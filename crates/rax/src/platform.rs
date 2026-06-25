//! Compile-time platform detection helpers.
//!
//! Exposes simple `const` booleans and a string so you can branch on platform
//! at compile time without cluttering your code with `cfg!` macros.
//!
//! # Example
//! ```rust
//! use rax::platform::{IS_IOS, IS_MACOS, PLATFORM};
//!
//! if IS_IOS {
//!     println!("Running on iOS");
//! }
//! println!("Platform: {PLATFORM}");
//! ```

/// `true` when targeting iOS (`target_os = "ios"`).
pub const IS_IOS: bool = cfg!(target_os = "ios");

/// `true` when targeting Android (`target_os = "android"`).
pub const IS_ANDROID: bool = cfg!(target_os = "android");

/// `true` when targeting macOS (`target_os = "macos"`).
pub const IS_MACOS: bool = cfg!(target_os = "macos");

/// `true` in debug builds (`cfg!(debug_assertions)`).
pub const IS_DEBUG: bool = cfg!(debug_assertions);

/// The current target platform as a lowercase string.
///
/// One of `"ios"`, `"android"`, `"macos"`, or `"unknown"`.
pub const PLATFORM: &str = if cfg!(target_os = "ios") {
    "ios"
} else if cfg!(target_os = "android") {
    "android"
} else if cfg!(target_os = "macos") {
    "macos"
} else {
    "unknown"
};

/// Returns one of two values depending on whether the current target is iOS.
///
/// This is a zero-cost helper: the unused branch is eliminated at compile time.
///
/// # Example
/// ```rust
/// use rax::platform::platform_value;
///
/// let padding: f32 = platform_value(16.0, 12.0); // 16 on iOS, 12 elsewhere
/// ```
#[inline(always)]
pub fn platform_value<T>(ios: T, android: T) -> T {
    #[cfg(target_os = "ios")]
    {
        let _ = android;
        ios
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = ios;
        android
    }
}
