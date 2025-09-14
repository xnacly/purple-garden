#pragma once

#include "common.h"
#include "parser.h"
#include "vm.h"

#ifndef DISASSEMBLE_INCLUDE_POSITIONS
#define DISASSEMBLE_INCLUDE_POSITIONS 0
#endif

#define BC(CODE, ARG) ByteCodeBuilder_add(ctx->bcb, CODE, ARG)
#define BC_LEN ctx->bcb->len

typedef enum {
  COMPILE_BUILTIN_UNKNOWN = 0,
  COMPILE_BUILTIN_LET,
  COMPILE_BUILTIN_FUNCTION,
  COMPILE_BUILTIN_ASSERT,
  COMPILE_BUILTIN_NONE,
  COMPILE_BUILTIN_MATCH,
} COMPILE_BUILTIN;

typedef struct CtxFunction {
  Str name;
  size_t size;
  size_t bytecode_index;
  size_t argument_count;
} CtxFunction;

// ByteCodeBuilder is used to efficiently build the buffer necessary for
// bytecode storage
typedef struct {
  Allocator *alloc;
  uint32_t *buffer;
  size_t cap;
  size_t len;
} ByteCodeBuilder;
ByteCodeBuilder ByteCodeBuilder_new(Allocator *a);
void ByteCodeBuilder_add(ByteCodeBuilder *bcb, uint32_t op, uint32_t arg);
void ByteCodeBuilder_insert_arg(ByteCodeBuilder *bcb, size_t idx, uint32_t arg);

// Used internally in the compiler to keep track of the currently allocated
// registers, what global slot is filled, what functions are defined by their
// hashes
typedef struct Ctx {
  bool registers[REGISTERS + 1];
  size_t global_hash_buckets[GLOBAL_SIZE];
  size_t register_allocated_count;
  CtxFunction hash_to_function[MAX_BUILTIN_SIZE];
  ByteCodeBuilder *bcb;
} Ctx;

// cc requests a Node from parser::Parser_next compiles said Node and its
// children to populate the Vm, its global pool, its bytecode and do all prep
// the runtime requires
Ctx cc(Vm *vm, Allocator *alloc, Parser *p);

// disassemble prints a readable bytecode representation with labels, globals
// and comments as a heap allocated string
void disassemble(const Vm *vm, const Ctx *ctx);

// stats displays some statistics around the bytecode
void bytecode_stats(const Vm *vm);
