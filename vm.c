#include "vm.h"
#include "common.h"

#if DEBUG
String OP_MAP[] = {
    [OP_LOAD] = STRING("OP_LOAD"), [OP_STORE] = STRING("OP_STORE"),
    [OP_ADD] = STRING("OP_ADD"),   [OP_SUB] = STRING("OP_SUB"),
    [OP_MUL] = STRING("OP_MUL"),   [OP_DIV] = STRING("OP_DIV"),
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

#if DEBUG
void Vm_Value_debug(Value *v) {
  if (v == NULL) {
    v->type = V_NULL;
  }
  String_debug(&VALUE_MAP[v->type]);
  switch (v->type) {
  case V_NULL:
  case V_TRUE:
  case V_FALSE:
    break;
  case V_STRING:
    printf("(`");
    String_debug(&v->string);
    printf("`)");
    break;
  case V_NUM:
    printf("(%f)", v->number);
    break;
  case V_LIST:
    TODO("Vm_Value_debug#V_LIST unimplemend")
  default:
    printf("<unkown>");
  }
  puts("");
}
#endif

int Vm_run(Vm *vm) {
#if DEBUG
  puts("================= GLOB =================");
  for (size_t i = 0; i < vm->global_len; i++) {
    printf("VM[glob%zu/%zu] ", i + 1, vm->global_len);
    Vm_Value_debug(&vm->globals[i]);
  }
  puts("================= VMOP =================");
#endif
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
    case OP_ADD:
      Value *a = &vm->_registers[0];
      Value *b = &vm->_registers[arg];
      VM_ASSERT(a->type == b->type, "VM[+] Incompatible type")
      switch (a->type) {
      case V_NUM:
        vm->_registers[0] = (Value){.type = V_NUM,
                                    .number = vm->_registers[0].number +
                                              vm->_registers[arg].number};
        break;
      case V_STRING:
        VM_ASSERT(0, "VM[+] String concat not implemented yet")
      default:
        VM_ASSERT(0, "VM[+] Only strings and numbers can be concatinated")
      }
      break;
    default:
      ASSERT(false, "Unimplemented instruction")
    }
    vm->_pc += 2;
  }
#if DEBUG
  puts("================= REGS =================");
#define REGISTER_PRINT_COUNT 3
  for (size_t i = 0; i < REGISTER_PRINT_COUNT; i++) {
    printf("VM[r%zu]: ", i);
    Vm_Value_debug(&vm->_registers[i]);
  }
#endif
  return 0;
vm_end:
  return 1;
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
// Edgecase: V_LIST & V_LIST is false, because we do a shallow compare
bool Vm_Value_cmp(Value a, Value b) {
  if (a.type != b.type) {
    return false;
  }

  switch (a.type) {
  case V_STRING:
    return String_eq(&a.string, &b.string);
  case V_NUM:
    // comparing doubles by subtracting them and comparing the result to an
    // epsilon is okay id say
    return vm_fabs(a.number - b.number) < PREC;
  case V_TRUE:
  case V_FALSE:
  case V_NULL:
    return true;
  case V_LIST:
  default:
    // lists arent really the same, this is not a deep equal
    return false;
  }
}

#undef PREC
