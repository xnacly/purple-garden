#include "../vm.h"

void builtin_opt_some(Vm *vm) {
  ASSERT(vm->arg_count == 1, "Some only works for a singular argument")
  Value inner = ARG(0);
  inner.is_some = true;
  RETURN(inner);
}

void builtin_opt_none(Vm *vm) { RETURN(*INTERNED_NONE); }
