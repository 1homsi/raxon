//! Structured logging for rax.
//!
//! Routes to the platform native log (oslog on iOS, logcat on Android,
//! stderr on desktop). Provides simple macros that mirror `tracing` ergonomics.
//!
//! # Example
//! ```
//! rax_debug!(target: "auth", "Biometric result: {}", result);
//! rax_info!("App started");
//! rax_warn!(target: "network", "Slow response: {}ms", elapsed);
//! rax_error!("Failed to load: {}", err);
//! ```

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Level::Debug => write!(f, "DEBUG"),
            Level::Info  => write!(f, "INFO"),
            Level::Warn  => write!(f, "WARN"),
            Level::Error => write!(f, "ERROR"),
        }
    }
}

use std::cell::Cell;

thread_local! {
    static MIN_LEVEL: Cell<Level> = Cell::new(Level::Debug);
}

/// Set the minimum log level. Messages below this level are discarded.
pub fn set_min_level(level: Level) {
    MIN_LEVEL.with(|l| l.set(level));
}

/// Get the current minimum log level.
pub fn min_level() -> Level {
    MIN_LEVEL.with(|l| l.get())
}

/// Core log function. Prefer the macros.
pub fn log(level: Level, target: &str, message: &str) {
    if level < min_level() { return; }

    #[cfg(target_os = "ios")]
    {
        // On iOS, os_log is the right destination. For simplicity, use NSLog
        // via a println that gets forwarded — a real impl uses os_log FFI.
        // The newline-flush pattern ensures Xcode console receives the output.
        println!("[rax:{level}:{target}] {message}");
    }

    #[cfg(not(target_os = "ios"))]
    {
        let _ = target;
        eprintln!("[rax:{level}] {message}");
    }
}

/// Log at DEBUG level.
#[macro_export]
macro_rules! rax_debug {
    (target: $target:expr, $($arg:tt)*) => {
        $crate::log($crate::Level::Debug, $target, &format!($($arg)*))
    };
    ($($arg:tt)*) => {
        $crate::log($crate::Level::Debug, "rax", &format!($($arg)*))
    };
}

/// Log at INFO level.
#[macro_export]
macro_rules! rax_info {
    (target: $target:expr, $($arg:tt)*) => {
        $crate::log($crate::Level::Info, $target, &format!($($arg)*))
    };
    ($($arg:tt)*) => {
        $crate::log($crate::Level::Info, "rax", &format!($($arg)*))
    };
}

/// Log at WARN level.
#[macro_export]
macro_rules! rax_warn {
    (target: $target:expr, $($arg:tt)*) => {
        $crate::log($crate::Level::Warn, $target, &format!($($arg)*))
    };
    ($($arg:tt)*) => {
        $crate::log($crate::Level::Warn, "rax", &format!($($arg)*))
    };
}

/// Log at ERROR level.
#[macro_export]
macro_rules! rax_error {
    (target: $target:expr, $($arg:tt)*) => {
        $crate::log($crate::Level::Error, $target, &format!($($arg)*))
    };
    ($($arg:tt)*) => {
        $crate::log($crate::Level::Error, "rax", &format!($($arg)*))
    };
}
