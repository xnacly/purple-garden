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

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
}

/// `QOS_CLASS_USER_INTERACTIVE` from `<sys/qos.h>`.
#[cfg(target_os = "macos")]
const QOS_CLASS_USER_INTERACTIVE: u32 = 0x21;

/// Ask the scheduler to prefer the fast cores for the calling thread: the user-interactive quality
/// of service, the class the scheduler places on the performance cores first. Like the Linux clamp
/// it leaves the efficiency cores usable when the fast ones are busy. Unlike it, only the calling
/// thread gets the hint: threads it creates afterwards start at the default class and processes at
/// user-interactive on their own. A process the system clamps lower (`taskpolicy -b`, a background
/// launchd job) stays clamped, and a thread opted out of quality of service with
/// `pthread_setschedparam` stays unchanged.
#[cfg(target_os = "macos")]
pub fn prefer_performance_cores() {
    // SAFETY: plain call on the calling thread, an invalid request returns an error and changes nothing
    unsafe { pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0) };
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::{
        QOS_CLASS_USER_INTERACTIVE, prefer_performance_cores, pthread_set_qos_class_self_np,
    };
    use std::ffi::{CStr, c_void};

    const QOS_CLASS_UNSPECIFIED: u32 = 0x00;
    const QOS_CLASS_BACKGROUND: u32 = 0x09;
    const QOS_CLASS_DEFAULT: u32 = 0x15;
    /// `THREAD_QOS_POLICY` and its tiers from xnu's `<mach/thread_policy.h>`, private in the SDK.
    const THREAD_QOS_POLICY: u32 = 9;
    const THREAD_QOS_BACKGROUND: i32 = 2;
    const THREAD_QOS_LEGACY: i32 = 4;
    const THREAD_QOS_USER_INTERACTIVE: i32 = 6;
    const SCHED_OTHER: i32 = 1;

    #[repr(C)]
    struct SchedParam {
        priority: i32,
        opaque: [u8; 4],
    }

    unsafe extern "C" {
        fn pthread_self() -> usize;
        fn pthread_get_qos_class_np(thread: usize, qos_class: *mut u32, relative: *mut i32) -> i32;
        fn pthread_mach_thread_np(thread: usize) -> u32;
        fn pthread_cpu_number_np(cpu: *mut usize) -> i32;
        fn pthread_getschedparam(thread: usize, policy: *mut i32, param: *mut SchedParam) -> i32;
        fn pthread_setschedparam(thread: usize, policy: i32, param: *const SchedParam) -> i32;
        fn thread_policy_get(
            thread: u32,
            flavor: u32,
            info: *mut i32,
            count: *mut u32,
            get_default: *mut u32,
        ) -> i32;
        fn sysctlbyname(
            name: *const i8,
            old: *mut c_void,
            old_len: *mut usize,
            new: *const c_void,
            new_len: usize,
        ) -> i32;
    }

    /// The class and relative priority the calling thread requested.
    fn requested() -> (u32, i32) {
        let (mut class, mut relative) = (0, 0);
        // SAFETY: both out pointers are live locals
        assert_eq!(0, unsafe {
            pthread_get_qos_class_np(pthread_self(), &mut class, &mut relative)
        });
        (class, relative)
    }

    /// The tier the kernel holds for the calling thread, independent of libpthread's bookkeeping.
    fn kernel_tier() -> i32 {
        let mut policy = [0i32; 2];
        let (mut count, mut get_default) = (2u32, 0u32);
        // SAFETY: policy has room for the count of integers stated, pthread_mach_thread_np borrows
        // the calling thread's port without taking a reference
        let kr = unsafe {
            thread_policy_get(
                pthread_mach_thread_np(pthread_self()),
                THREAD_QOS_POLICY,
                policy.as_mut_ptr(),
                &mut count,
                &mut get_default,
            )
        };
        assert_eq!(0, kr);
        policy[0]
    }

    fn set_class(class: u32, relative: i32) {
        // SAFETY: plain call on the calling thread
        assert_eq!(0, unsafe { pthread_set_qos_class_self_np(class, relative) });
    }

    fn sysctl_u32(name: &CStr) -> u32 {
        let mut value = 0u32;
        let mut len = size_of::<u32>();
        // SAFETY: value has room for the len stated
        let rc = unsafe {
            sysctlbyname(
                name.as_ptr(),
                (&raw mut value).cast(),
                &mut len,
                std::ptr::null(),
                0,
            )
        };
        assert_eq!(0, rc, "sysctl {name:?}");
        value
    }

    /// The cpu the calling thread runs on after each of `samples` short bursts of work.
    fn cpus_used(samples: usize) -> Vec<usize> {
        (0..samples)
            .map(|_| {
                let mut x = 0u64;
                for i in 0..100_000u64 {
                    x = std::hint::black_box(x.wrapping_add(i));
                }
                let mut cpu = 0;
                // SAFETY: cpu is a live local
                assert_eq!(0, unsafe { pthread_cpu_number_np(&mut cpu) });
                cpu
            })
            .collect()
    }

    /// Run `f` on a thread of its own, so it starts at the class every new thread gets and its
    /// changes leave the other tests alone. The result comes back over a channel: a thread waited on
    /// with `pthread_join` runs at least at the class of the thread waiting for it.
    fn on_new_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || tx.send(f()));
        rx.recv().expect("test thread panicked")
    }

    #[test]
    fn raises_a_new_thread_to_user_interactive() {
        on_new_thread(|| {
            assert_eq!((QOS_CLASS_DEFAULT, 0), requested());
            assert_eq!(THREAD_QOS_LEGACY, kernel_tier());
            prefer_performance_cores();
            assert_eq!((QOS_CLASS_USER_INTERACTIVE, 0), requested());
            assert_eq!(THREAD_QOS_USER_INTERACTIVE, kernel_tier());
        });
    }

    #[test]
    fn lifts_a_thread_lowered_to_background() {
        on_new_thread(|| {
            set_class(QOS_CLASS_BACKGROUND, 0);
            assert_eq!(THREAD_QOS_BACKGROUND, kernel_tier());
            prefer_performance_cores();
            assert_eq!((QOS_CLASS_USER_INTERACTIVE, 0), requested());
            assert_eq!(THREAD_QOS_USER_INTERACTIVE, kernel_tier());
        });
    }

    #[test]
    fn resets_a_lowered_relative_priority_and_repeats_cleanly() {
        on_new_thread(|| {
            set_class(QOS_CLASS_USER_INTERACTIVE, -15);
            assert_eq!((QOS_CLASS_USER_INTERACTIVE, -15), requested());
            prefer_performance_cores();
            assert_eq!((QOS_CLASS_USER_INTERACTIVE, 0), requested());
            prefer_performance_cores();
            assert_eq!((QOS_CLASS_USER_INTERACTIVE, 0), requested());
            assert_eq!(THREAD_QOS_USER_INTERACTIVE, kernel_tier());
        });
    }

    #[test]
    fn stays_on_the_calling_thread() {
        let (spawned, caller) = on_new_thread(|| {
            prefer_performance_cores();
            let spawned = on_new_thread(|| (requested(), kernel_tier()));
            (spawned, requested())
        });
        assert_eq!(((QOS_CLASS_DEFAULT, 0), THREAD_QOS_LEGACY), spawned);
        assert_eq!((QOS_CLASS_USER_INTERACTIVE, 0), caller);
    }

    #[test]
    fn leaves_a_thread_opted_out_of_qos_unchanged() {
        on_new_thread(|| {
            let mut policy = 0;
            let mut param = SchedParam {
                priority: 0,
                opaque: [0; 4],
            };
            // SAFETY: plain calls on the calling thread with live locals
            unsafe {
                assert_eq!(
                    0,
                    pthread_getschedparam(pthread_self(), &mut policy, &mut param)
                );
                assert_eq!(
                    0,
                    pthread_setschedparam(pthread_self(), SCHED_OTHER, &param)
                );
            }
            assert_eq!(QOS_CLASS_UNSPECIFIED, requested().0);
            prefer_performance_cores();
            assert_eq!(QOS_CLASS_UNSPECIFIED, requested().0);
        });
    }

    /// The placement the hint is for: a background thread runs on the efficiency cores only, so the
    /// cpus it is seen on are efficiency cores, and after the hint the thread runs elsewhere.
    #[test]
    fn moves_a_thread_off_the_efficiency_cores() {
        if sysctl_u32(c"hw.nperflevels") < 2 {
            return;
        }
        let efficiency_cores = sysctl_u32(c"hw.perflevel1.logicalcpu") as usize;
        on_new_thread(move || {
            set_class(QOS_CLASS_BACKGROUND, 0);
            let mut efficiency = cpus_used(400);
            efficiency.sort_unstable();
            efficiency.dedup();
            assert!(
                efficiency.len() <= efficiency_cores,
                "background thread ran on {efficiency:?}, more cpus than the {efficiency_cores} efficiency cores"
            );

            prefer_performance_cores();
            cpus_used(50);
            let after = cpus_used(400);
            let on_performance = after.iter().filter(|c| !efficiency.contains(c)).count();
            assert!(
                on_performance * 2 > after.len(),
                "only {on_performance} of {} samples left the efficiency cores {efficiency:?}: {after:?}",
                after.len()
            );
        });
    }
}
