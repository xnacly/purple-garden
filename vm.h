#pragma once

#include "builtins.h"
#include "common.h"
#include "parser.h"
#include <stdint.h>
#include <stdlib.h>

#define REGISTERS 127
#define CALL_ARGUMENT_STACK 256
#define VARIABLE_TABLE_SIZE 256
#define VARIABLE_TABLE_SIZE_MASK (VARIABLE_TABLE_SIZE - 1)

static Value *INTERNED_TRUE = &(Value){.type = V_TRUE};
static Value *INTERNED_FALSE = &(Value){.type = V_FALSE};
static Value *INTERNED_NONE =
    &(Value){.type = V_OPTION, .option = (struct Option){.is_some = false}};

typedef enum {
  // LOAD rANY
  //
  // LOAD a Value from the const table to r0
  OP_LOAD,
  // STORE rANY
  //
  // STORE a Value from r0 into an arbitrary register
  OP_STORE,
  // OP_VAR globalANY
  //
  // Copy value from Frame assigned to variable name stored in global pool at
  // globalANY
  // OP_VAR,

  // OP_ADD rANY
  //
  // add Value at rANY to r0, store result in r0
  OP_ADD = 2,
  // OP_SUB rANY
  //
  // subtract Value at rANY from r0, store result in r0
  OP_SUB = 3,
  // OP_MUL rANY
  //
  // multiply Value at rANY with r0, store result in r0
  OP_MUL = 4,
  // OP_DIV rANY
  //
  // divide Value at rANY with r0, store result in r0
  OP_DIV = 5,

  // OP_EQ rANY
  //
  // compares value at r0 and rANY via Value_cmp
  OP_EQ = 6,

  // OP_PUSH rANY
  //
  // pushes the value in rANY to the stack
  OP_PUSH,

  // OP_PUSHG gANY
  //
  // pushes a value from the global pool to the stack
  OP_PUSHG,

  // OP_POP rANY
  //
  // pops a value from the stack
  OP_POP,

  // OP_VAR rANY
  //
  // stores the Value in the register rANY in the variable table via the
  // identifier stored in r0
  OP_VAR,

  // OP_LOADV hash
  //
  // loads the Value stored in the variable table by hash
  OP_LOADV,

  // OP_ARGS aANY
  //
  // instructs the vm on how many values to pop of the argument stacks
  OP_ARGS,

  // OP_BUILTIN bANY
  //
  // call the builtin its argument refers to, with the argument stored in r0
  OP_BUILTIN,

  // OP_RET rANY
  //
  // Ends a scope
  OP_LEAVE,

  // OP_CALL ADDR
  //
  // 1: enters a new stackframe, stores the last vm->pc in
  // Frame.return_to_bytecode
  //
  // 2: jumps to ADDR
  OP_CALL,

  // OP_JMP bc
  //
  // Jumps to bc in bytecode index, does no bounds checking
  OP_JMP,

  // OP_ASSERT
  //
  // stops execution with error message if r0 evals to false
  OP_ASSERT,
} VM_OP;

#define VM_ERR(fmt, ...)                                                       \
  fprintf(stderr, "[VM] ERROR: " fmt "\n", ##__VA_ARGS__);                     \
  goto vm_end;

extern Str OP_MAP[];

// A frame represents a Scope, a new scope is created upon entering a lambda -
// since lambdas are pure there is no way to interact with the previous frame
// inside of a lambda, the pointer is kept to allow the runtime to restore the
// scope state to its state before entering the lambda
//
// WARNING: do not stack allocate, since variable_table can be huge
typedef struct Frame {
  struct Frame *prev;
  // returning out of scope, we need to jump back to the callsite of the
  // function
  size_t return_to_bytecode;
  // stores Values by their hash, serving as a variable table
  Value *variable_table[VARIABLE_TABLE_SIZE];
} Frame;

typedef struct {
  uint32_t global_len;
  // globals represents the global pool created by the bytecode compiler
  Value **globals;
  uint64_t bytecode_len;
  uint32_t *bytecode;
  // current position in the bytecode
  size_t pc;
  Value registers[REGISTERS + 1];
  // frame stores variables of the current scope, meta data and other required
  // data
  Frame *frame;
  // stack is used for handling arguments of a function or builtin call, if more
  // than one arguments are passed to a function these are pushed to the stack,
  // except the last one, which is in r0 either way, so we just take it from
  // there
  Value stack[CALL_ARGUMENT_STACK];
  // stack_cur stores how many elements there currently are in the stack
  size_t stack_cur;
  // arg_count enables the vm to know how many register values it needs to pop
  // off the stack and pass to the function called via CALL or BUILTIN
  size_t arg_count;

  builtin_function *builtins;
#if DEBUG
  size_t instruction_counter[256];
#endif
} Vm;

Vm Vm_new(Allocator *alloc);
void Vm_register_builtin(Vm *vm, builtin_function bf, Str name);
int Vm_run(Vm *vm, Allocator *alloc);
void Vm_destroy(Vm *vm);
