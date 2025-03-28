#include "vm.h"
#include "common.h"

#if DEBUG
String OP_MAP[] = {
    [OP_LOAD] = STRING("OP_LOAD"),
    [OP_STORE] = STRING("OP_STORE"),
    [OP_ADD] = STRING("OP_ADD"),
};

String VALUE_MAP[] = {
    [V_NULL] = STRING("V_NULL"),   [V_STRING] = STRING("V_STRING"),
    [V_NUM] = STRING("V_NUM"),     [V_TRUE] = STRING("V_TRUE"),
    [V_FALSE] = STRING("V_FALSE"), [V_LIST] = STRING("V_LIST"),
};
#endif

#define VM_ASSERT(expr, msg)                                                   \
  if (!(expr)) {                                                               \
    fprintf(stderr,                                                            \
            "[VM] ASSERT(" #expr "): `" msg "` failed at %s, line %d\n",       \
            __FILE__, __LINE__);                                               \
    goto vm_end;                                                               \
  }

void Vm_run(Vm *vm) {
  while (vm->_pc < vm->bytecode_len) {
    VM_OP op = vm->bytecode[vm->_pc];
    size_t arg = vm->bytecode[vm->_pc + 1];
    // #if DEBUG
    //     DIS(op, arg)
    // #endif
    switch (op) {
    case OP_LOAD:
      vm->_registers[0] = vm->globals[arg];
      break;
    case OP_STORE:
      vm->_registers[arg] = vm->_registers[0];
      break;
    case OP_ADD:
      Value *a = &vm->_registers[0];
      Value *b = &vm->_registers[arg];
      VM_ASSERT(a->type != V_NUM || b->type != V_NUM,
                "Bad types for OP_ADD, cant add anything other than ")
      vm->_registers[0] = (Value){.type = V_NUM,
                                  .number = vm->_registers[0].number +
                                            vm->_registers[arg].number};
      break;
    // case OP_VAR:
    //   TODO("OP_VAR is not implemented yet, because Frame is not implemented "
    //        "AND because HASHMAPS arent implemented")
    default:
      ASSERT(false, "Unimplemented instruction")
    }
    vm->_pc += 2;
  }
vm_end:
}

void Vm_destroy(Vm vm) {
  free(vm.globals);
  free(vm.bytecode);
}

#define PREC 1e7

static double vm_fabs(double x) { return x < 0 ? -x : x; }

// Vm_Value_cmp compares two values in a shallow way, is used for OP_EQ and in
// tests.
//
// Edgecases:
//
// 1. V_NULL & V_NULL is false
// 2. V_LIST & V_LIST is false, because we do a shallow compare
bool Vm_Value_cmp(Value a, Value b) {
  if (a.type != b.type) {
    return false;
  }

  switch (a.type) {
  case V_NULL:
    return false;
  case V_STRING:
    return String_eq(&a.string, &b.string);
  case V_NUM:
    // comparing doubles by subtracting them and comparing the result to an
    // epsilon is okay id say
    return vm_fabs(a.number - b.number) < PREC;
  case V_TRUE:
  case V_FALSE:
    return true;
  case V_LIST:
  default:
    // lists arent really the same, this is not a deep equal
    return false;
  }
}

#undef PREC
