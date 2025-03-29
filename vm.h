#ifndef VM_H
#define VM_H

#include "parser.h"
#include <stdlib.h>

#define REGISTERS 128

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
  OP_DIV
} VM_OP;

typedef enum {
  V_NULL,
  V_STRING,
  V_NUM,
  V_TRUE,
  V_FALSE,
  V_LIST,
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
  size_t global_cap;
  // globals represents the global pool created by the bytecode compiler
  Value *globals;

  size_t bytecode_len;
  size_t bytecode_cap;
  byte *bytecode;

  size_t _pc;
  Value _registers[REGISTERS + 1];
} Vm;

int Vm_run(Vm *vm);
void Vm_destroy(Vm vm);
bool Vm_Value_cmp(Value a, Value b);
#if DEBUG
void Vm_Value_debug(Value *v);
#endif

#endif
