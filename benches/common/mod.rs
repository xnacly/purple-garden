use std::time::Duration;

use criterion::Criterion;

pub fn quick_requested() -> bool {
    std::env::var_os("PG_BENCH_QUICK").is_some()
}

pub fn criterion() -> Criterion {
    let criterion = Criterion::default();

    if quick_requested() {
        eprintln!(
            "running quick Criterion profile: sample_size=10 warm_up=100ms measurement=300ms"
        );
        criterion
            .sample_size(10)
            .warm_up_time(Duration::from_millis(100))
            .measurement_time(Duration::from_millis(300))
            .nresamples(1_000)
    } else {
        criterion.configure_from_args()
    }
}
