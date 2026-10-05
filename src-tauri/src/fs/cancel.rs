use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Shared registry of in-flight downloads keyed by catalog/engine id.
#[derive(Default)]
pub struct DownloadCanceller {
    active: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl DownloadCanceller {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new download. Replaces any previous flag for the same id.
    pub fn begin(&self, id: &str) -> Result<Arc<AtomicBool>, String> {
        let flag = Arc::new(AtomicBool::new(false));
        let mut guard = self
            .active
            .lock()
            .map_err(|_| "cancel_lock_poisoned".to_string())?;
        guard.insert(id.to_string(), Arc::clone(&flag));
        Ok(flag)
    }

    pub fn request_cancel(&self, id: &str) -> Result<bool, String> {
        let guard = self
            .active
            .lock()
            .map_err(|_| "cancel_lock_poisoned".to_string())?;
        if let Some(flag) = guard.get(id) {
            flag.store(true, Ordering::SeqCst);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn finish(&self, id: &str) {
        if let Ok(mut guard) = self.active.lock() {
            guard.remove(id);
        }
    }
}

pub fn is_cancelled(flag: &AtomicBool) -> bool {
    flag.load(Ordering::SeqCst)
}

pub const CANCELLED: &str = "cancelled";
