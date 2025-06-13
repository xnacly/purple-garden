#pragma once

#include "builtins.h"
#include "common.h"
#include "parser.h"
#include <stdint.h>
#include <stdlib.h>

static Value *INTERNED_TRUE = &(Value){.type = V_TRUE};
static Value *INTERNED_FALSE = &(Value){.type = V_FALSE};
static Value *INTERNED_NONE =
    &(Value){.type = V_OPTION, .option = (struct Option){.is_some = false}};

typedef enum {

  // STORE rANY
  //
  // STORE a Value from r0 into an arbitrary register
  OP_STORE,

  // LOAD rANY
  //
  // LOAD a Value from rANY into r0
  OP_LOAD,

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

  // LOADG rANY
  //
  // LOADG a global from the const table to r0
  OP_LOADG,
} VM_OP;

#define VM_ERR(fmt, ...)                                                       \
  fprintf(stderr, "[VM] ERROR: " fmt "\n", ##__VA_ARGS__);                     \
  goto vm_end;

extern Str OP_MAP[];

Vm Vm_new(Allocator *alloc);
void Vm_register_builtin(Vm *vm, builtin_function bf, Str name);
int Vm_run(Vm *vm);
void Vm_destroy(Vm *vm);
