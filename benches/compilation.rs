use criterion::{Criterion, criterion_group, criterion_main};
use purple_garden::{
    self,
    config::{self, Config},
};

const CODE_AMOUNT: usize = 5000;
const CONFIG: &config::Config = &Config::default();
const CONFIG_OPT: &config::Config = &{
    let mut c = Config::default();
    c.opt = 1;
    c
};
const FACTORIAL: &'static str = "
fn factorial(n:int a:int) int {
    match {
        n == 0 { a }
        { factorial(n-1 n*a) }
    }
}
factorial(20 1)
    ";

/// benchmark compilation
pub fn bench_compilation(c: &mut Criterion) {
    let factorial = FACTORIAL.repeat(CODE_AMOUNT);
    c.bench_function("bench_compilation", |b| {
        b.iter(|| {
            let _ = purple_garden::new(CONFIG, factorial.as_bytes()).unwrap();
        })
    });
}

pub fn bench_compilation_opt(c: &mut Criterion) {
    let factorial = FACTORIAL.repeat(CODE_AMOUNT);
    c.bench_function("bench_compilation_opt", |b| {
        b.iter(|| {
            let _ = purple_garden::new(CONFIG_OPT, factorial.as_bytes()).unwrap();
        })
    });
}

// this is a stupid micro benchmark
pub fn bench_compilation_factorial(c: &mut Criterion) {
    c.bench_function("bench_compilation_factorial", |b| {
        b.iter(|| {
            let _ = purple_garden::new(CONFIG, FACTORIAL.as_bytes()).unwrap();
        })
    });
}

criterion_group!(
    compilation,
    bench_compilation,
    bench_compilation_opt,
    bench_compilation_factorial
);
criterion_main!(compilation);
