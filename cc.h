#pragma once

#include "common.h"
#include "parser.h"
#include "vm.h"

#ifndef DISASSEMBLE_INCLUDE_POSITIONS
#define DISASSEMBLE_INCLUDE_POSITIONS 0
#endif

#define BC(CODE, ARG)                                                          \
  vm->bytecode[vm->bytecode_len++] = CODE;                                     \
  vm->bytecode[vm->bytecode_len++] = ARG;                                      \
  ASSERT(vm->bytecode_len <= BYTECODE_SIZE,                                    \
         "cc: out of bytecode space, what the fuck are you doing");

#define GROW_FACTOR 2

typedef enum {
  COMPILE_BUILTIN_UNKNOWN = 0,
  COMPILE_BUILTIN_LET,
  COMPILE_BUILTIN_FUNCTION,
  COMPILE_BUILTIN_ASSERT,
  COMPILE_BUILTIN_NONE,
  COMPILE_BUILTIN_MATCH,
} COMPILE_BUILTIN;

typedef struct CtxFunction {
  Str *name;
  size_t size;
  size_t bytecode_index;
  size_t argument_count;
} CtxFunction;

typedef struct Ctx {
  bool registers[REGISTERS + 1];
  size_t *global_hash_buckets;
  size_t register_allocated_count;
  CtxFunction hash_to_function[MAX_BUILTIN_SIZE];
} Ctx;

// cc requests a Node from parser::Parser_next compiles said Node and its
// children to populate the Vm, its global pool, its bytecode and do all prep
// the runtime requires
Ctx cc(Vm *vm, Allocator *alloc, Node **nodes, size_t size);

// Like cc(), but allows seeding the compiler context with a previous context
// to enable incremental compilation (e.g., for a REPL) so that previously
// defined functions remain callable in subsequent compilations.
Ctx cc_seeded(Vm *vm, Allocator *alloc, Node **nodes, size_t size,
              const Ctx *seed);

// disassemble prints a readable bytecode representation with labels, globals
// and comments as a heap allocated string
void disassemble(const Vm *vm, const Ctx *ctx);

// stats displays some statistics around the bytecode
void bytecode_stats(const Vm *vm);
