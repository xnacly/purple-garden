#ifndef PGCC_H
#define PGCC_H

#include "vm.h"

#ifndef DISASSEMBLE_INCLUDE_POSITIONS
#define DISASSEMBLE_INCLUDE_POSITIONS 0
#endif

#define BC(CODE, ARG)                                                          \
  vm->bytecode[vm->bytecode_len++] = CODE;                                     \
  vm->bytecode[vm->bytecode_len++] = ARG;                                      \
  ASSERT(vm->bytecode_len <= BYTECODE_SIZE,                                    \
         "cc: out of bytecode space, what the fuck are you doing (there is "   \
         "space for 4MB of bytecode)");

#define GROW_FACTOR 2
#define MAX_BUILTIN_SIZE 1024
#define MAX_BUILTIN_SIZE_MASK (MAX_BUILTIN_SIZE - 1)

typedef enum {
  COMPILE_BUILTIN_LET = 256,
  COMPILE_BUILTIN_FUNCTION,
} COMPILE_BUILTIN;

typedef struct Ctx {
  bool registers[REGISTERS + 1];
  size_t *global_hash_buckets;
  size_t register_allocated_count;
  int *function_hash_to_bytecode_index;
  Str *function_hash_to_function_name;
} Ctx;

typedef struct {
  Vm vm;
  Ctx ctx;
} CompileOutput;

// cc requests a Node from parser::Parser_next compiles said Node and its
// children to populate the Vm, its global pool, its bytecode and do all prep
// the runtime requires
CompileOutput cc(Allocator *alloc, Node **nodes, size_t size);

// disassemble prints a readable bytecode representation with labels, globals
// and comments as a heap allocated string
void disassemble(const Vm *vm, const Ctx *ctx);

#endif
