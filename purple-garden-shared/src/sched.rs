//! Placement of the interpreter on the CPU's fast cores.

#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn syscall(number: i64, ...) -> i64;
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const SYS_SCHED_SETATTR: i64 = 314;
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const SYS_SCHED_SETATTR: i64 = 274;

/// The kernel's `struct sched_attr` up to the utilization clamps.
#[cfg(target_os = "linux")]
#[repr(C)]
struct SchedAttr {
    size: u32,
    policy: u32,
    flags: u64,
    nice: i32,
    priority: u32,
    runtime: u64,
    deadline: u64,
    period: u64,
    util_min: u32,
    util_max: u32,
}

/// Ask the scheduler to prefer the fast cores for the calling thread, and the threads and processes
/// it creates afterwards: a minimum utilization clamp at its maximum, the hint `uclampset -m 1024`
/// gives. Unlike an affinity it leaves the slow cores usable when the fast ones are busy, and on a
/// kernel without utilization clamping nothing changes.
#[cfg(target_os = "linux")]
pub fn prefer_performance_cores() {
    const KEEP_POLICY: u64 = 0x08;
    const KEEP_PARAMS: u64 = 0x10;
    const UTIL_CLAMP_MIN: u64 = 0x20;
    let attr = SchedAttr {
        size: size_of::<SchedAttr>() as u32,
        policy: 0,
        flags: KEEP_POLICY | KEEP_PARAMS | UTIL_CLAMP_MIN,
        nice: 0,
        priority: 0,
        runtime: 0,
        deadline: 0,
        period: 0,
        util_min: 1024,
        util_max: 0,
    };
    // SAFETY: attr is a sched_attr of the size it states, pid 0 is the calling thread
    unsafe { syscall(SYS_SCHED_SETATTR, 0i32, &raw const attr, 0u32) };
}

#[cfg(not(target_os = "linux"))]
pub fn prefer_performance_cores() {}
