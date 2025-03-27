#include "vm.h"
#include "common.h"

#if DEBUG
String OP_MAP[] = {
    [OP_LOAD] = STRING("OP_LOAD"),
    [OP_STORE] = STRING("OP_STORE"),
    [OP_VAR] = STRING("OP_VAR"),
};
#endif

void Vm_run(Vm *vm) {
  while (vm->_pc < vm->bytecode_len) {
    VM_OP op = vm->bytecode[vm->_pc];
    size_t arg = vm->bytecode[vm->_pc + 1];
#if DEBUG
    DIS(op, arg)
#endif
    switch (op) {
    case OP_LOAD:
      vm->_registers[0] = vm->globals[arg];
      break;
    case OP_STORE:
      vm->_registers[arg] = vm->_registers[0];
      break;
    case OP_VAR:
      TODO("OP_VAR is not implemented yet, because Frame is not implemented "
           "AND because HASHMAPS arent implemented")
    default:
      ASSERT(false, "Unimplemented instruction")
    }
    vm->_pc += 2;
  }
}

void Vm_destroy(Vm vm) {
  free(vm.globals);
  free(vm.bytecode);
}
