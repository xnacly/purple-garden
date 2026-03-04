use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use purple_garden::{
    self, config,
    vm::{Vm, op::Op},
};
use rand::{RngExt, SeedableRng, rngs::StdRng};

const OP_CODE_SIZE: usize = 10_000_000;
static CONFIG: config::Config = config::Config::default();

/// benchmark pure virtual machine dispatch / throughput with 10 million Nop's
pub fn bench_uniform_dispatch(c: &mut Criterion) {
    c.bench_function("bench_uniform_dispatch", |b| {
        b.iter_batched(
            || {
                let mut bc = Vec::with_capacity(OP_CODE_SIZE);
                for _ in 0..OP_CODE_SIZE {
                    bc.push(purple_garden::vm::op::Op::Nop);
                }
                let mut v = Vm::new(&CONFIG);
                v.bytecode = bc;
                v
            },
            |mut vm| vm.run(),
            BatchSize::SmallInput,
        )
    });
}

/// benchmark virtual machine dispatch with random ops
pub fn bench_random_dispatch(c: &mut Criterion) {
    const RANDOM_OPS: &[Op] = &[
        Op::IAdd {
            dst: 0,
            lhs: 0,
            rhs: 0,
        },
        Op::ISub {
            dst: 0,
            lhs: 0,
            rhs: 0,
        },
        Op::IMul {
            dst: 0,
            lhs: 0,
            rhs: 0,
        },
        Op::IDiv {
            dst: 0,
            lhs: 0,
            rhs: 0,
        },
        Op::DAdd {
            dst: 0,
            lhs: 0,
            rhs: 0,
        },
        Op::DSub {
            dst: 0,
            lhs: 0,
            rhs: 0,
        },
        Op::DMul {
            dst: 0,
            lhs: 0,
            rhs: 0,
        },
        Op::DDiv {
            dst: 0,
            lhs: 0,
            rhs: 0,
        },
        Op::ILt {
            dst: 0,
            lhs: 0,
            rhs: 1,
        },
        Op::IGt {
            dst: 0,
            lhs: 0,
            rhs: 0,
        },
        Op::DLt {
            dst: 0,
            lhs: 0,
            rhs: 0,
        },
        Op::DGt {
            dst: 0,
            lhs: 0,
            rhs: 0,
        },
        Op::IEq {
            dst: 0,
            lhs: 0,
            rhs: 0,
        },
        Op::BEq {
            dst: 0,
            lhs: 0,
            rhs: 0,
        },
        Op::Mov { dst: 0, src: 0 },
        Op::LoadI { dst: 0, value: 0 },
        Op::CastToInt { dst: 0, src: 0 },
        Op::CastToDouble { dst: 0, src: 0 },
        Op::CastToBool { dst: 0, src: 0 },
        Op::Nop,
    ];

    c.bench_function("bench_random_dispatch", |b| {
        b.iter_batched(
            || {
                let mut bc = Vec::with_capacity(OP_CODE_SIZE);
                let mut rng = StdRng::seed_from_u64(0);
                for _ in 0..OP_CODE_SIZE {
                    let idx = rng.random_range(0..RANDOM_OPS.len());
                    bc.push(RANDOM_OPS[idx].clone());
                }
                let mut v = Vm::new(&CONFIG);
                v.bytecode = bc;
                v
            },
            |mut vm| vm.run(),
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(dispatch, bench_uniform_dispatch, bench_random_dispatch);
criterion_main!(dispatch);
