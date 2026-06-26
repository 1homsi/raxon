//! Persisted signals — automatically save/restore values via a KV store.
//!
//! The persisted value is encoded as a string via `Display`/`FromStr`.
//!
//! # Example
//! ```no_run
//! use rax_reactive::persisted_signal;
//! // Creates a signal backed by persistent storage under key "theme_mode":
//! let theme = persisted_signal("theme_mode", "light");
//! // When changed, the new value is saved immediately.
//! theme.set("dark".to_string());
//! // On next app launch, the value is restored from storage.
//! ```

use std::cell::RefCell;
use std::collections::HashMap;

use crate::{create_effect, create_signal, Signal};

// In-memory KV store backing persisted signals.
// In production, this should be bridged to the platform's UserDefaults / SharedPreferences.
thread_local! {
    static KV_STORE: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

/// Write a value to the KV store under `key`.
pub fn kv_set(key: &str, value: &str) {
    KV_STORE.with(|s| s.borrow_mut().insert(key.to_string(), value.to_string()));
}

/// Read a value from the KV store.
pub fn kv_get(key: &str) -> Option<String> {
    KV_STORE.with(|s| s.borrow().get(key).cloned())
}

/// Create a `Signal<String>` whose value is persisted across app sessions
/// under `key`. If a stored value exists, it is used as the initial value;
/// otherwise `default` is used. Changes automatically update the KV store.
pub fn persisted_signal(key: &'static str, default: &str) -> Signal<String> {
    let initial = kv_get(key).unwrap_or_else(|| default.to_string());
    let sig = create_signal(initial);
    create_effect(move || {
        let value = sig.get();
        kv_set(key, &value);
    });
    sig
}

/// Create a `Signal<bool>` backed by persistent storage.
pub fn persisted_bool(key: &'static str, default: bool) -> Signal<bool> {
    let initial = kv_get(key)
        .and_then(|s| s.parse().ok())
        .unwrap_or(default);
    let sig = create_signal(initial);
    create_effect(move || {
        kv_set(key, &sig.get().to_string());
    });
    sig
}

/// Create a `Signal<i64>` backed by persistent storage.
pub fn persisted_i64(key: &'static str, default: i64) -> Signal<i64> {
    let initial = kv_get(key)
        .and_then(|s| s.parse().ok())
        .unwrap_or(default);
    let sig = create_signal(initial);
    create_effect(move || {
        kv_set(key, &sig.get().to_string());
    });
    sig
}

/// Create a `Signal<f64>` backed by persistent storage.
pub fn persisted_f64(key: &'static str, default: f64) -> Signal<f64> {
    let initial = kv_get(key)
        .and_then(|s| s.parse().ok())
        .unwrap_or(default);
    let sig = create_signal(initial);
    create_effect(move || {
        kv_set(key, &sig.get().to_string());
    });
    sig
}

// ---------------------------------------------------------------------------
// Namespaced KV store
// ---------------------------------------------------------------------------

/// A namespaced view into the KV store. All keys are prefixed with `"<ns>."`.
///
/// # Example
/// ```rust,ignore
/// let user_store = KvNamespace::new("user");
/// user_store.set("name", "Alice");
/// assert_eq!(user_store.get("name"), Some("Alice".to_string()));
/// ```
#[derive(Clone, Debug)]
pub struct KvNamespace {
    prefix: String,
}

impl KvNamespace {
    /// Create a new namespace. Keys will be stored as `"<namespace>.<key>"`.
    pub fn new(namespace: &str) -> Self {
        Self { prefix: format!("{}.", namespace) }
    }

    fn full_key(&self, key: &str) -> String {
        format!("{}{}", self.prefix, key)
    }

    /// Write a value under `key` within this namespace.
    pub fn set(&self, key: &str, value: &str) {
        kv_set(&self.full_key(key), value);
    }

    /// Read a value under `key` within this namespace.
    pub fn get(&self, key: &str) -> Option<String> {
        kv_get(&self.full_key(key))
    }

    /// Remove a value.
    pub fn remove(&self, key: &str) {
        KV_STORE.with(|s| s.borrow_mut().remove(&self.full_key(key)));
    }

    /// List all keys in this namespace (without the prefix).
    pub fn keys(&self) -> Vec<String> {
        KV_STORE.with(|s| {
            s.borrow()
                .keys()
                .filter(|k| k.starts_with(&self.prefix))
                .map(|k| k[self.prefix.len()..].to_string())
                .collect()
        })
    }

    /// Clear all keys in this namespace.
    pub fn clear(&self) {
        let to_remove: Vec<String> = KV_STORE.with(|s| {
            s.borrow()
                .keys()
                .filter(|k| k.starts_with(&self.prefix))
                .cloned()
                .collect()
        });
        KV_STORE.with(|s| {
            let mut store = s.borrow_mut();
            for k in to_remove { store.remove(&k); }
        });
    }

    /// Create a `Signal<String>` persisted within this namespace under `key`.
    pub fn persisted(&self, key: &str, default: &str) -> Signal<String> {
        let full = self.full_key(key);
        let initial = kv_get(&full).unwrap_or_else(|| default.to_string());
        let sig = create_signal(initial);
        let full_key = full.clone();
        create_effect(move || {
            kv_set(&full_key, &sig.get());
        });
        sig
    }
}

// ---------------------------------------------------------------------------
// Reactive queries on the KV store
// ---------------------------------------------------------------------------

/// Watch a KV key for changes, returning a reactive `Signal<Option<String>>`.
///
/// The signal updates whenever `kv_set` is called with the same key via
/// `kv_set_reactive`. Use `kv_set_reactive` instead of `kv_set` to get
/// reactive updates.
///
/// # Example
/// ```rust,ignore
/// let name = watch_kv("user.name");
/// create_effect(move || {
///     if let Some(n) = name.get() { println!("name changed: {n}"); }
/// });
/// kv_set_reactive("user.name", "Alice"); // triggers effect
/// ```
thread_local! {
    static KV_WATCHERS: RefCell<HashMap<String, Signal<Option<String>>>> =
        RefCell::new(HashMap::new());
}

/// Write a value and notify any reactive watcher created via [`watch_kv`].
pub fn kv_set_reactive(key: &str, value: &str) {
    kv_set(key, value);
    KV_WATCHERS.with(|w| {
        if let Some(sig) = w.borrow().get(key) {
            sig.set(Some(value.to_string()));
        }
    });
}

/// Remove a reactive value and notify watchers.
pub fn kv_delete_reactive(key: &str) {
    KV_STORE.with(|s| s.borrow_mut().remove(key));
    KV_WATCHERS.with(|w| {
        if let Some(sig) = w.borrow().get(key) {
            sig.set(None);
        }
    });
}

/// Create or retrieve a reactive `Signal<Option<String>>` that tracks `key`.
pub fn watch_kv(key: &str) -> Signal<Option<String>> {
    KV_WATCHERS.with(|w| {
        let existing = w.borrow().get(key).copied();
        if let Some(sig) = existing {
            return sig;
        }
        let initial = kv_get(key);
        let sig = create_signal(initial);
        w.borrow_mut().insert(key.to_string(), sig);
        sig
    })
}
