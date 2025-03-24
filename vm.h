#ifndef VM_H
#define VM_H

#include "parser.h"
#include <stdlib.h>

// A frame represents a Scope, a new scope is created upon entering a lambda -
// since lambdas are pure there is no way to interact with the previous frame
// inside of a lambda, the pointer is kept to allow the runtime to restore the
// scope state to its state before entering the lambda
typedef struct {
  struct Frame *prev;
} Frame;

typedef enum {
  // LOAD a Value from the const table to r0
  OP_LOAD,
  // STORE a Value from r0 into an arbitrary register
  OP_STORE,

  // TODO: CALL a function by jumping to its location in the bytecode and
  // entering a
  // new frame
  // CALL,

  // EXIT a child frame and enter the previous frame
  // RETURN,
} VM_OP;

typedef enum {
  V_NULL,
  V_STRING,
  V_NUM,
  V_TRUE,
  V_FALSE,
  V_LIST,
  // TODO: V_OBJECT,
  // TODO: V_LAMBDA, this should probably just be a jump to a different bc index
  // via B_INVOKE
} ValueType;

// Value represents a value known to the runtime
typedef struct {
  ValueType type;
  union {
    String string;
    double number;
  };
} Value;

typedef unsigned short byte;

typedef struct {
  size_t global_len;
  // globals represents the global pool created by the bytecode compiler
  Value *globals;
  size_t bytecode_len;
  byte *bytecode;
  size_t _pc;
  Value _registers[64];
} Vm;

void Vm_run(Vm *vm);
void Vm_destroy(Vm vm);

#endif
