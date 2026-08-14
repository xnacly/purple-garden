use criterion::{BatchSize, Criterion};
use purple_garden_runtime::{DEFAULT_STACK_SIZE, Vm, VmConfig, op::Op};
use rand::{RngExt, SeedableRng, rngs::StdRng};

mod common;

const OP_CODE_SIZE: usize = 10_000_000;
static CONFIG: VmConfig = VmConfig {
    backtrace: false,
    no_gc: false,
    stack_size: DEFAULT_STACK_SIZE * 4,
};

/// benchmark pure virtual machine dispatch / throughput with 10 million Nop's
pub fn bench_uniform_dispatch(c: &mut Criterion) {
    c.bench_function("bench_uniform_dispatch", |b| {
        b.iter_batched(
            || {
                let mut bc = Vec::with_capacity(OP_CODE_SIZE);
                for _ in 0..OP_CODE_SIZE {
                    bc.push(Op::Nop);
                }
                let mut vm = Vm::new(CONFIG);
                vm.bytecode = bc;
                vm
            },
            |mut vm| vm.run::<false>(&[]),
            BatchSize::LargeInput,
        );
    });
}

/// benchmark virtual machine dispatch with random ops
pub fn bench_random_dispatch(c: &mut Criterion) {
    const RANDOM_OPS: &[Op] = &[
        Op::IAdd {
            dst: 2,
            lhs: 0,
            rhs: 0,
        },
        Op::ISub {
            dst: 2,
            lhs: 0,
            rhs: 0,
        },
        Op::IMul {
            dst: 2,
            lhs: 0,
            rhs: 0,
        },
        Op::DAdd {
            dst: 2,
            lhs: 0,
            rhs: 0,
        },
        Op::DSub {
            dst: 2,
            lhs: 0,
            rhs: 0,
        },
        Op::DMul {
            dst: 2,
            lhs: 0,
            rhs: 0,
        },
        Op::ILt {
            dst: 2,
            lhs: 0,
            rhs: 1,
        },
        Op::IGt {
            dst: 2,
            lhs: 0,
            rhs: 0,
        },
        Op::DLt {
            dst: 2,
            lhs: 0,
            rhs: 0,
        },
        Op::DGt {
            dst: 2,
            lhs: 0,
            rhs: 0,
        },
        Op::IEq {
            dst: 2,
            lhs: 0,
            rhs: 0,
        },
        Op::BEq {
            dst: 2,
            lhs: 0,
            rhs: 0,
        },
        Op::Mov { dst: 2, src: 0 },
        Op::LoadI { dst: 2, value: 0 },
        Op::CastToInt { dst: 2, src: 0 },
        Op::CastToDouble { dst: 2, src: 0 },
        Op::CastToBool { dst: 2, src: 0 },
        Op::Nop,
    ];

    c.bench_function("bench_random_dispatch", |b| {
        b.iter_batched(
            || {
                let mut bc = Vec::with_capacity(OP_CODE_SIZE);
                bc.push(Op::LoadI { dst: 0, value: 1 });
                bc.push(Op::LoadI { dst: 1, value: 2 });
                let mut rng = StdRng::seed_from_u64(0);
                for _ in 2..OP_CODE_SIZE {
                    let idx = rng.random_range(0..RANDOM_OPS.len());
                    bc.push(RANDOM_OPS[idx]);
                }
                let mut vm = Vm::new(CONFIG);
                vm.bytecode = bc;
                vm
            },
            |mut vm| vm.run::<false>(&[]),
            BatchSize::LargeInput,
        );
    });
}

/// benchmark the call frame stack: 10 million Call/Ret pairs into a leaf fn
pub fn bench_call_dispatch(c: &mut Criterion) {
    c.bench_function("bench_call_dispatch", |b| {
        b.iter_batched(
            || {
                let leaf = OP_CODE_SIZE as u32;
                let mut bc = Vec::with_capacity(OP_CODE_SIZE + 1);
                for _ in 0..OP_CODE_SIZE {
                    bc.push(Op::Call { func: leaf });
                }
                bc.push(Op::Ret);
                let mut vm = Vm::new(CONFIG);
                vm.bytecode = bc;
                vm
            },
            |mut vm| vm.run::<false>(&[]),
            BatchSize::LargeInput,
        );
    });
}

pub fn bench_packed_spill_dispatch(c: &mut Criterion) {
    for (name, push, pop) in [
        (
            "bench_push2_pop2_dispatch",
            Op::Push2 { a: 0, b: 1 },
            Op::Pop2 { a: 2, b: 3 },
        ),
        (
            "bench_push3_pop3_dispatch",
            Op::Push3 { a: 0, b: 1, c: 2 },
            Op::Pop3 { a: 3, b: 4, c: 5 },
        ),
    ] {
        c.bench_function(name, |b| {
            b.iter_batched(
                || {
                    let mut bc = Vec::with_capacity(OP_CODE_SIZE);
                    for _ in 0..OP_CODE_SIZE / 2 {
                        bc.push(push);
                        bc.push(pop);
                    }
                    let mut vm = Vm::new(CONFIG);
                    vm.bytecode = bc;
                    vm
                },
                |mut vm| vm.run::<false>(&[]),
                BatchSize::LargeInput,
            );
        });
    }
}

fn main() {
    let mut criterion = common::criterion();
    bench_uniform_dispatch(&mut criterion);
    bench_random_dispatch(&mut criterion);
    bench_call_dispatch(&mut criterion);
    bench_packed_spill_dispatch(&mut criterion);
    criterion.final_summary();
}
