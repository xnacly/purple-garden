#include "vm.h"
#include "builtins.h"
#include "common.h"

#if DEBUG
String OP_MAP[] = {
    [OP_LOAD] = STRING("OP_LOAD"),      [OP_STORE] = STRING("OP_STORE"),
    [OP_ADD] = STRING("OP_ADD"),        [OP_SUB] = STRING("OP_SUB"),
    [OP_MUL] = STRING("OP_MUL"),        [OP_DIV] = STRING("OP_DIV"),
    [OP_BUILTIN] = STRING("OP_BUILTIN")};
#endif

String VALUE_TYPE_MAP[] = {
    [V_OPTION] = STRING("Option("), [V_STRING] = STRING("String"),
    [V_NUM] = STRING("Number"),     [V_TRUE] = STRING("True"),
    [V_FALSE] = STRING("False"),    [V_LIST] = STRING("List"),
};

#define VM_ASSERT(expr, msg)                                                   \
  if (!(expr)) {                                                               \
    fprintf(stderr,                                                            \
            "[VM] ASSERT(" #expr "): `" msg "` failed at %s, line %d\n",       \
            __FILE__, __LINE__);                                               \
    goto vm_end;                                                               \
  }

#if DEBUG
void Vm_Value_debug(Value *v) {
  String_debug(&VALUE_TYPE_MAP[v->type]);
  switch (v->type) {
  case V_OPTION: {
    if (v->option.is_some) {
      printf("Some(");
      Vm_Value_debug(v->option.value);
      printf(")");
    } else {
      printf("None");
    }
    putc(')', stdout);
    break;
  }
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
  case V_UNDEFINED:
    printf("undefined");
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
  while (vm->pc < vm->bytecode_len) {
    VM_OP op = vm->bytecode[vm->pc];
    size_t arg = vm->bytecode[vm->pc + 1];
#if DEBUG
    DIS(op, arg)
#endif
    switch (op) {
    case OP_LOAD:
      vm->registers[0] = vm->globals[arg];
      break;
    case OP_STORE:
      vm->registers[arg] = vm->registers[0];
      break;
    case OP_ADD:
      Value *a = &vm->registers[0];
      Value *b = &vm->registers[arg];
      VM_ASSERT(a->type == b->type, "VM[+] Incompatible type")
      switch (a->type) {
      case V_NUM:
        vm->registers[0] = (Value){.type = V_NUM,
                                   .number = vm->registers[0].number +
                                             vm->registers[arg].number};
        break;
      case V_STRING:
        VM_ASSERT(0, "VM[+] String concat not implemented yet")
      default:
        VM_ASSERT(0, "VM[+] Only strings and numbers can be concatinated")
      }
      break;
    case OP_BUILTIN: {
      // TODO: more checks here if we would handle some builtins differently
      // from just calling a function
      vm->registers[0] = BUILTIN_MAP[arg](&vm->registers[0]);
      break;
    }
    default:
      ASSERT(false, "Unimplemented instruction")
    }
    vm->pc += 2;
  }
#if DEBUG
  puts("================= REGS =================");
#define REGISTER_PRINT_COUNT 3
  for (size_t i = 0; i < REGISTER_PRINT_COUNT; i++) {
    printf("VM[r%zu]: ", i);
    Vm_Value_debug(&vm->registers[i]);
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
