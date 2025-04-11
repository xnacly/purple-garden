#ifndef VM_H
#define VM_H

#include "parser.h"
#include <stdlib.h>

#define REGISTERS 128

#define DIS(op, arg)                                                           \
  printf("VM[%06zu][%-8.*s][%3zu]: ", vm->pc, (int)OP_MAP[(op)].len,           \
         OP_MAP[(op)].p, arg);                                                 \
  for (size_t i = 0; i < 3; i++) {                                             \
    Value_debug(&vm->registers[i]);                                            \
    if (i < 2) {                                                               \
      printf(", ");                                                            \
    }                                                                          \
  }                                                                            \
  printf("\n");

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
  OP_ADD,
  // OP_SUB rANY
  //
  // subtract Value at rANY from r0, store result in r0
  OP_SUB,
  // OP_MUL rANY
  //
  // multiply Value at rANY with r0, store result in r0
  OP_MUL,
  // OP_DIV rANY
  //
  // divide Value at rANY with r0, store result in r0
  OP_DIV,

  // OP_ARGS aANY
  //
  // defines how many arguments the following function or builtin call will
  // accept
  OP_ARGS,
  // OP_OFFSET aANY
  //
  // defines where the arguments to a function or builtin call start
  OP_OFFSET,

  // OP_BUILTIN bANY
  //
  // call the builtin its argument refers to, with the argument stored in r0
  OP_BUILTIN,
} VM_OP;

extern Str OP_MAP[];

typedef unsigned short byte;

// A frame represents a Scope, a new scope is created upon entering a lambda -
// since lambdas are pure there is no way to interact with the previous frame
// inside of a lambda, the pointer is kept to allow the runtime to restore the
// scope state to its state before entering the lambda
typedef struct {
  struct Frame *prev;
  // returning out of scope, we need to jump back to the callsite of the
  // function
  size_t return_to_bytecode;
} Frame;

typedef struct {
  size_t global_len;
  // globals represents the global pool created by the bytecode compiler
  Value *globals;

  size_t bytecode_len;
  byte *bytecode;

  // current position in the bytecode
  size_t pc;

  Value registers[REGISTERS + 1];
  // frame stores variables of the current scope, meta data and other required
  // data
  Frame frame;

  // arg_count enables the vm to know how many registers it should pass to the
  // function called via CALL or BUILTIN
  size_t arg_count;
  // register_offset enables the vm to know where to start with collecting
  // values out of registers
  size_t register_offset;
} Vm;

int Vm_run(Vm *vm);
void Vm_destroy(Vm vm);
void Value_debug(const Value *v);

#endif
