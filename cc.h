#ifndef PGCC_H
#define PGCC_H

#include "vm.h"

typedef struct Ctx {
  bool registers[REGISTERS + 1];
  size_t *global_hash_buckets;
  size_t register_allocated_count;
  int *function_hash_to_bytecode_index;
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
