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

const FIB: &str = "
fn fib(n:int) int {
    match {
        n == 0 { 0 }
        n == 1 { 1 }
        { fib(n - 1) + fib(n - 2) }
    }
}
fib(28)
    ";

const FIBT: &str = "
fn fibt(n:int a:int b:int) int {
    match {
        n == 0 { a }
        n == 1 { b }
        { fibt(n-1 b a+b) }
    }
}
fibt(50 0 1)
    ";

pub fn bench_fib(c: &mut Criterion) {
    c.bench_function("bench_fib", |b| {
        let mut vm = purple_garden::new(CONFIG, FIB.as_bytes()).unwrap();
        let entry = vm.pc;
        b.iter(|| {
            vm.pc = entry;
            vm.run()
        })
    });
}

pub fn bench_fib_opt(c: &mut Criterion) {
    c.bench_function("bench_fib_opt", |b| {
        let mut vm = purple_garden::new(CONFIG_OPT, FIB.as_bytes()).unwrap();
        let entry = vm.pc;
        b.iter(|| {
            vm.pc = entry;
            vm.run()
        })
    });
}

pub fn bench_fibt(c: &mut Criterion) {
    c.bench_function("bench_fibt", |b| {
        let mut vm = purple_garden::new(CONFIG, FIBT.as_bytes()).unwrap();
        let entry = vm.pc;
        b.iter(|| {
            vm.pc = entry;
            vm.run()
        })
    });
}

pub fn bench_fibt_opt(c: &mut Criterion) {
    c.bench_function("bench_fibt_opt", |b| {
        let mut vm = purple_garden::new(CONFIG_OPT, FIBT.as_bytes()).unwrap();
        let entry = vm.pc;
        b.iter(|| {
            vm.pc = entry;
            vm.run()
        })
    });
}

criterion_group!(fib, bench_fib, bench_fib_opt, bench_fibt, bench_fibt_opt);
criterion_main!(fib);
