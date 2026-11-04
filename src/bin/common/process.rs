//! What this process costs the machine, for the runner's phase logging.
//!
//! The memory budget is an estimate; these readings are what lets an operator
//! check it against reality without an external profiler.

use std::time::{Duration, Instant};

/// Current resident set size in bytes, if the platform exposes it.
pub fn resident_set_size() -> Option<u64> {
    platform::resident_set_size()
}

/// Peak resident set size in bytes since the process started.
pub fn peak_resident_set_size() -> Option<u64> {
    platform::peak_resident_set_size()
}

/// Render a byte count the way the configuration would have written it.
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [(&str, u64); 3] = [("GB", 1_000_000_000), ("MB", 1_000_000), ("kB", 1_000)];
    for (unit, scale) in UNITS {
        if bytes >= scale {
            return format!("{:.2} {unit}", bytes as f64 / scale as f64);
        }
    }
    format!("{bytes} B")
}

/// Current and peak resident size, rendered for a log line.
pub fn describe_memory() -> String {
    match (resident_set_size(), peak_resident_set_size()) {
        (Some(current), Some(peak)) => {
            format!(
                "rss {} (peak {})",
                format_bytes(current),
                format_bytes(peak)
            )
        }
        (Some(current), None) => format!("rss {}", format_bytes(current)),
        (None, Some(peak)) => format!("peak rss {}", format_bytes(peak)),
        (None, None) => String::from("rss unavailable"),
    }
}

/// Wall-clock time split over the named phases of a run.
///
/// Phases are measured one at a time, as the runner executes them, and summed
/// so the end of a model can say where its time went.
#[derive(Debug, Default)]
pub struct PhaseTimer {
    phases: Vec<(&'static str, Duration)>,
}

impl PhaseTimer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Run `work` as `phase`, add its duration to the total, and return it.
    pub fn time<T>(&mut self, phase: &'static str, work: impl FnOnce() -> T) -> (T, Duration) {
        let start = Instant::now();
        let result = work();
        let elapsed = start.elapsed();
        self.add(phase, elapsed);
        (result, elapsed)
    }

    pub fn add(&mut self, phase: &'static str, elapsed: Duration) {
        match self.phases.iter_mut().find(|(name, _)| *name == phase) {
            Some((_, total)) => *total += elapsed,
            None => self.phases.push((phase, elapsed)),
        }
    }

    /// `step 12.3s, postprocess 4.5s` for a log line.
    pub fn summary(&self) -> String {
        self.phases
            .iter()
            .map(|(name, total)| format!("{name} {:.1}s", total.as_secs_f64()))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(target_os = "linux")]
mod platform {
    pub fn resident_set_size() -> Option<u64> {
        // `/proc/self/statm`: size resident shared text lib data dt, in pages.
        let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
        let resident_pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
        // SAFETY: sysconf has no preconditions.
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if page_size <= 0 {
            return None;
        }
        Some(resident_pages * page_size as u64)
    }

    pub fn peak_resident_set_size() -> Option<u64> {
        // Linux reports ru_maxrss in kilobytes.
        super::max_rss_raw().map(|raw| raw * 1024)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    pub fn resident_set_size() -> Option<u64> {
        let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
        // SAFETY: the buffer is a correctly sized, writable `proc_taskinfo`,
        // and the pid is our own.
        let written = unsafe {
            libc::proc_pidinfo(
                std::process::id() as libc::c_int,
                libc::PROC_PIDTASKINFO,
                0,
                (&mut info as *mut libc::proc_taskinfo).cast(),
                size,
            )
        };
        (written == size).then_some(info.pti_resident_size)
    }

    pub fn peak_resident_set_size() -> Option<u64> {
        // macOS reports ru_maxrss in bytes.
        super::max_rss_raw()
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod platform {
    pub fn resident_set_size() -> Option<u64> {
        None
    }

    pub fn peak_resident_set_size() -> Option<u64> {
        None
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn max_rss_raw() -> Option<u64> {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: `usage` is a valid, writable `rusage`.
    let status = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    (status == 0).then(|| u64::try_from(usage.ru_maxrss).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_rendered_in_the_configuration_units() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1_500), "1.50 kB");
        assert_eq!(format_bytes(6_000_000_000), "6.00 GB");
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn the_process_can_read_its_own_memory() {
        // Touch a buffer so the numbers cannot be trivially zero.
        let buffer = vec![1_u8; 8 << 20];
        let current = resident_set_size().expect("rss is readable");
        let peak = peak_resident_set_size().expect("peak rss is readable");
        assert!(current >= buffer.len() as u64);
        assert!(
            peak >= current / 2,
            "peak {peak} is far below current {current}"
        );
    }

    #[test]
    fn phases_accumulate_and_summarize_in_first_seen_order() {
        let mut timer = PhaseTimer::new();
        timer.add("step", Duration::from_millis(1500));
        timer.add("postprocess", Duration::from_millis(250));
        timer.add("step", Duration::from_millis(500));
        let (value, elapsed) = timer.time("warm state", || 7);
        assert_eq!(value, 7);
        assert!(elapsed < Duration::from_secs(1));
        assert!(timer
            .summary()
            .starts_with("step 2.0s, postprocess 0.2s, warm state "));
    }
}
