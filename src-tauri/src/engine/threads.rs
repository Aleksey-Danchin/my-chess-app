//! Cap engine Threads at half of available logical CPUs (at least 1).

pub fn cpu_count() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4)
        .max(1)
}

/// Hard ceiling: never more than half the logical processors.
pub fn max_allowed_threads() -> u32 {
    (cpu_count() / 2).max(1)
}

pub fn clamp_threads(requested: u32) -> u32 {
    requested.clamp(1, max_allowed_threads())
}
