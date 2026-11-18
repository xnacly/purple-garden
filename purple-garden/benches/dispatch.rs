use criterion::{BatchSize, Criterion};
use purple_garden_runtime::{Check, DEFAULT_STACK_SIZE, Vm, VmConfig, gc::AllocType, op::Op};
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
                let mut bc = Vec::with_capacity(OP_CODE_SIZE + 1);
                for _ in 0..OP_CODE_SIZE {
                    bc.push(Op::Nop);
                }
                bc.push(Op::Halt);
                let mut vm = Vm::new(CONFIG);
                vm.bytecode = bc.into_iter().map(Op::encode).collect();
                vm
            },
            |mut vm| vm.run::<false>(&[]),
            BatchSize::LargeInput,
        );
    });
}

/// benchmark virtual machine dispatch with random non control flow ops
pub fn bench_random_dispatch(c: &mut Criterion) {
    // r0 = 1, r1 = 2, r2 = scratch, r3 = two word array, r4 = true, r5 = 1.0, r6 = 2.0, one
    // spilled slot
    const SETUP: &[Op] = &[
        Op::LoadI { dst: 0, value: 1 },
        Op::LoadI { dst: 1, value: 2 },
        Op::Alloc {
            dst: 3,
            kind: AllocType::Array,
            size: 16,
            align: 8,
        },
        Op::Store {
            base: 3,
            offset: 0,
            src: 0,
        },
        Op::LoadI { dst: 4, value: 1 },
        Op::CastToDouble { dst: 5, src: 0 },
        Op::CastToDouble { dst: 6, src: 1 },
        Op::Push { src: 0 },
    ];
    const RANDOM_OPS: &[Op] = &[
        Op::IAdd {
            dst: 2,
            lhs: 0,
            rhs: 1,
        },
        Op::ISub {
            dst: 2,
            lhs: 0,
            rhs: 1,
        },
        Op::IMul {
            dst: 2,
            lhs: 0,
            rhs: 1,
        },
        Op::IDiv {
            dst: 2,
            lhs: 0,
            rhs: 1,
        },
        Op::IMod {
            dst: 2,
            lhs: 0,
            rhs: 1,
        },
        Op::IAnd {
            dst: 2,
            lhs: 0,
            rhs: 1,
        },
        Op::ILt {
            dst: 2,
            lhs: 0,
            rhs: 1,
        },
        Op::IGt {
            dst: 2,
            lhs: 0,
            rhs: 1,
        },
        Op::IEq {
            dst: 2,
            lhs: 0,
            rhs: 1,
        },
        Op::IAddI {
            dst: 2,
            lhs: 0,
            imm: 1,
        },
        Op::ISubI {
            dst: 2,
            lhs: 0,
            imm: 1,
        },
        Op::IMulI {
            dst: 2,
            lhs: 0,
            imm: 8,
        },
        Op::IDivI {
            dst: 2,
            lhs: 0,
            imm: 2,
        },
        Op::IModI {
            dst: 2,
            lhs: 0,
            imm: 3,
        },
        Op::IAndI {
            dst: 2,
            lhs: 0,
            imm: 1,
        },
        Op::IEqI {
            dst: 2,
            lhs: 0,
            imm: 1,
        },
        Op::IGtI {
            dst: 2,
            lhs: 0,
            imm: -1,
        },
        Op::ILtI {
            dst: 2,
            lhs: 0,
            imm: 32,
        },
        Op::DAdd {
            dst: 2,
            lhs: 5,
            rhs: 6,
        },
        Op::DSub {
            dst: 2,
            lhs: 5,
            rhs: 6,
        },
        Op::DMul {
            dst: 2,
            lhs: 5,
            rhs: 6,
        },
        Op::DDiv {
            dst: 2,
            lhs: 5,
            rhs: 6,
        },
        Op::DLt {
            dst: 2,
            lhs: 5,
            rhs: 6,
        },
        Op::DGt {
            dst: 2,
            lhs: 5,
            rhs: 6,
        },
        Op::DLe {
            dst: 2,
            lhs: 5,
            rhs: 6,
        },
        Op::DGe {
            dst: 2,
            lhs: 5,
            rhs: 6,
        },
        Op::Mov { dst: 2, src: 0 },
        Op::LoadI { dst: 2, value: 0 },
        Op::CastToInt { dst: 2, src: 5 },
        Op::CastToDouble { dst: 2, src: 0 },
        Op::CastToBool { dst: 2, src: 0 },
        Op::Load {
            dst: 2,
            base: 3,
            offset: 8,
        },
        Op::Store {
            base: 3,
            offset: 8,
            src: 0,
        },
        Op::AddrOf {
            dst: 2,
            base: 3,
            offset: 8,
        },
        Op::Assert {
            cond: 4,
            check: Check::InBounds,
        },
        Op::LoadSlot { dst: 2, slot: 0 },
        Op::StoreSlot { slot: 0, src: 0 },
        Op::Nop,
    ];

    c.bench_function("bench_random_dispatch", |b| {
        b.iter_batched(
            || {
                let mut bc = Vec::with_capacity(OP_CODE_SIZE + 1);
                bc.extend_from_slice(SETUP);
                let mut rng = StdRng::seed_from_u64(0);
                for _ in SETUP.len()..OP_CODE_SIZE {
                    let idx = rng.random_range(0..RANDOM_OPS.len());
                    bc.push(RANDOM_OPS[idx]);
                }
                bc.push(Op::Halt);
                let mut vm = Vm::new(CONFIG);
                vm.bytecode = bc.into_iter().map(Op::encode).collect();
                vm
            },
            |mut vm| vm.run::<false>(&[]).unwrap(),
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
                let mut bc = Vec::with_capacity(OP_CODE_SIZE + 2);
                for _ in 0..OP_CODE_SIZE {
                    bc.push(Op::Call { func: leaf });
                }
                bc.push(Op::Ret);
                bc.push(Op::Halt);
                let mut vm = Vm::new(CONFIG);
                vm.bytecode = bc.into_iter().map(Op::encode).collect();
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
                    let mut bc = Vec::with_capacity(OP_CODE_SIZE + 1);
                    for _ in 0..OP_CODE_SIZE / 2 {
                        bc.push(push);
                        bc.push(pop);
                    }
                    bc.push(Op::Halt);
                    let mut vm = Vm::new(CONFIG);
                    vm.bytecode = bc.into_iter().map(Op::encode).collect();
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
