use criterion::{Criterion, criterion_group, criterion_main};
use purple_garden::{
    self,
    config::{self, Config},
};

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

pub fn bench_factorial(c: &mut Criterion) {
    c.bench_function("bench_factorial", |b| {
        let mut vm = purple_garden::new(CONFIG, FACTORIAL.as_bytes()).unwrap();
        let entry = vm.pc;
        b.iter(|| {
            vm.pc = entry;
            vm.run()
        })
    });
}

pub fn bench_factorial_opt(c: &mut Criterion) {
    c.bench_function("bench_factorial_opt", |b| {
        let mut vm = purple_garden::new(CONFIG_OPT, FACTORIAL.as_bytes()).unwrap();
        let entry = vm.pc;
        b.iter(|| {
            vm.pc = entry;
            vm.run()
        })
    });
}

criterion_group!(functions, bench_factorial, bench_factorial_opt);
criterion_main!(functions);
