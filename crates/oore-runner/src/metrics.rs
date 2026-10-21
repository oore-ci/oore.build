//! Host utilisation reported with each runner heartbeat.

use std::path::Path;

use oore_contract::RunnerMetrics;

/// Sample CPU, memory and the free space of the volume holding `workspace_root`.
pub fn sample_runner_metrics(workspace_root: &Path, cache_bytes: Option<u64>) -> RunnerMetrics {
    let (disk_free_bytes, disk_total_bytes) = disk_space(workspace_root).unwrap_or((0, 0));
    let (memory_used_bytes, memory_total_bytes) = memory().unwrap_or((0, 0));
    RunnerMetrics {
        cpu_percent: cpu_percent().unwrap_or(0.0),
        memory_used_bytes,
        memory_total_bytes,
        disk_free_bytes,
        disk_total_bytes,
        cache_bytes,
        collected_at: None,
    }
}

fn cpu_percent() -> Option<f64> {
    let mut load = [0f64; 3];
    // SAFETY: `load` has room for the three samples requested.
    let samples = unsafe { libc::getloadavg(load.as_mut_ptr(), 3) };
    if samples < 1 {
        return None;
    }
    let cpus = std::thread::available_parallelism().ok()?.get() as f64;
    Some((load[0] / cpus * 100.0).clamp(0.0, 100.0))
}

fn disk_space(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::ffi::OsStrExt;

    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: `statvfs` is plain data and fully written on success.
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `path` is a valid NUL-terminated string and `stats` is writable.
    if unsafe { libc::statvfs(path.as_ptr(), &mut stats) } != 0 {
        return None;
    }
    let block = stats.f_frsize as u64;
    Some((
        (stats.f_bavail as u64).saturating_mul(block),
        (stats.f_blocks as u64).saturating_mul(block),
    ))
}

#[cfg(target_os = "macos")]
fn memory() -> Option<(u64, u64)> {
    let mut total = 0u64;
    let mut size = std::mem::size_of::<u64>();
    // SAFETY: `hw.memsize` is a u64 and `size` matches the buffer.
    let status = unsafe {
        libc::sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&mut total as *mut u64).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        return None;
    }
    let output = std::process::Command::new("vm_stat").output().ok()?;
    let used = parse_vm_stat_used_bytes(&String::from_utf8_lossy(&output.stdout))?;
    Some((used.min(total), total))
}

#[cfg(not(target_os = "macos"))]
fn memory() -> Option<(u64, u64)> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let field = |name: &str| {
        meminfo
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| {
                rest.trim()
                    .trim_end_matches("kB")
                    .trim()
                    .parse::<u64>()
                    .ok()
            })
            .map(|kib| kib * 1024)
    };
    let total = field("MemTotal:")?;
    let available = field("MemAvailable:")?;
    Some((total.saturating_sub(available), total))
}

/// Memory in use as Activity Monitor counts it: active, wired and compressed pages.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_vm_stat_used_bytes(output: &str) -> Option<u64> {
    let page_size = output
        .lines()
        .next()?
        .split("page size of ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?;
    let pages = |name: &str| {
        output
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| rest.trim().trim_end_matches('.').parse::<u64>().ok())
    };
    let used = pages("Pages active:")?
        + pages("Pages wired down:")?
        + pages("Pages occupied by compressor:").unwrap_or(0);
    Some(used * page_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vm_stat_used_memory_counts_active_wired_and_compressed_pages() {
        let output = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\n\
Pages free:                               10000.\n\
Pages active:                            100.\n\
Pages inactive:                          5000.\n\
Pages wired down:                         20.\n\
Pages occupied by compressor:              3.\n";
        assert_eq!(parse_vm_stat_used_bytes(output), Some(123 * 16384));
    }

    #[test]
    fn samples_disk_space_for_an_existing_volume() {
        let metrics = sample_runner_metrics(&std::env::temp_dir(), Some(7));
        assert!(metrics.disk_total_bytes > 0);
        assert!(metrics.disk_free_bytes <= metrics.disk_total_bytes);
        assert_eq!(metrics.cache_bytes, Some(7));
    }
}
