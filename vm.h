#ifndef VM_H
#define VM_H

#include "parser.h"
#include <stdlib.h>

// A frame represents a Scope, a new scope is created upon entering a lambda -
// since lambdas are pure there is no way to interact with the previous frame
// inside of a lambda, the pointer is kept to allow the runtime to restore the
// scope state to its state before entering the lambda
typedef struct Frame {
  Frame *prev;
} Frame;

enum ValueType {
  V_STRING,
  V_NUM,
  V_TRUE,
  V_FALSE,
  V_LIST,
  // TODO: V_OBJECT,
  // TODO: V_LAMBDA, this should probably just be a jump to a different bc index
  // via B_INVOKE
};

// Value represents a value known to the runtime
typedef struct {
  ValueType type;
  union {
    String string;
    double number;
  };
} Value;

typedef struct {
} Pool;

typedef unsigned short byte;

typedef struct {
  size_t bc_len;
  // globals represents the global pool created by the bytecode compiler
  Value globals[1024];
  byte *bc;
} Vm;

Vm Vm_new(byte *bc, size_t len, Pool globals);
void Vm_run(Vm vm);

#endif
