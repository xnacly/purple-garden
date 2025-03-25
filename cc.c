#include "cc.h"
#include "vm.h"

#define TODO(msg) ASSERT(0, msg)
#define BC(CODE, ARG)                                                          \
  {                                                                            \
    vm->bytecode[vm->bytecode_len++] = CODE;                                   \
    vm->bytecode[vm->bytecode_len++] = ARG;                                    \
  }

static void compile(Vm *vm, Node *n) {
  // INFO: this is an example for a simple simple interaction the compiler could
  // do
  //
  // vm->globals = malloc(sizeof(Value));
  // vm->globals[0] = (Value){.type = V_STRING, .string = STRING("hello
  // world")}; vm->bytecode = malloc(sizeof(byte) * 4); BC(OP_LOAD, 0)
  // BC(OP_STORE, 25)

  switch (n->type) {
  case N_ATOM:
    TODO("N_ATOM is not implemented");
  case N_IDENT:
    TODO("N_IDENT is not implemented");
  case N_LIST:
    TODO("N_LIST is not implemented");
  case N_LAMBDA:
    TODO("N_LAMBDA is not implemented");
  case N_OP:
    TODO("N_OP is not implemented");
  case N_UNKOWN:
  default:
    TODO("N_UNKOWN is no a known Node to compile, sorry");
    break;
  }
}

Vm cc(Node *n) {
  Vm vm = {.global_len = 0,
           .bytecode_len = 0,
           ._pc = 0,
           .bytecode = NULL,
           .globals = NULL};
  compile(&vm, n);
  return vm;
}

#undef BC
#undef TODO
