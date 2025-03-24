#include "cc.h"
#include "vm.h"
#include <stdlib.h>

#define BC(CODE, ARG)                                                          \
  {                                                                            \
    vm.bytecode[vm.bytecode_len++] = CODE;                                     \
    vm.bytecode[vm.bytecode_len++] = ARG;                                      \
  }

Vm cc(Node n) {
  Vm vm = {.global_len = 1, .bytecode_len = 0, ._pc = 0};

  // TODO: replace this with the real implementation - this is only a test
  vm.globals = malloc(sizeof(Value));
  vm.globals[0] = (Value){.type = V_STRING, .string = STRING("hello world")};
  vm.bytecode = malloc(sizeof(byte) * 4);
  BC(OP_LOAD, 0)
  BC(OP_STORE, 25)
  return vm;
}

#undef BC
