#ifndef VM_H
#define VM_H

#include "parser.h"
#include <stdlib.h>

#define DIS(op, arg)                                                           \
  printf("VM[%06zu(%06zu)] %s(%zu)\n", vm->_pc, vm->_pc + 1, OP_MAP[(op)].p,   \
         (arg));

// A frame represents a Scope, a new scope is created upon entering a lambda -
// since lambdas are pure there is no way to interact with the previous frame
// inside of a lambda, the pointer is kept to allow the runtime to restore the
// scope state to its state before entering the lambda
typedef struct {
  struct Frame *prev;
} Frame;

typedef enum {
  // LOAD rANY
  //
  // LOAD a Value from the const table to r0
  OP_LOAD,
  // STORE rANY
  //
  // STORE a Value from r0 into an arbitrary register
  //
  // TODO: (IDEA) should this remove the value at r0?
  OP_STORE,
  // OP_VAR rANY
  //
  // Copy value from Frame assigned to variable name stored in rANY
  OP_VAR,
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

#if DEBUG
extern String OP_MAP[];
extern String VALUE_MAP[];
#endif

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
bool Vm_Value_cmp(Value a, Value b);

#endif
