#include "vm.h"
#include "builtins.h"
#include "common.h"
#include "strings.h"

Str OP_MAP[] = {[OP_LOAD] = STRING("LOAD"),      [OP_STORE] = STRING("STORE"),
                [OP_ADD] = STRING("ADD"),        [OP_SUB] = STRING("SUB"),
                [OP_MUL] = STRING("MUL"),        [OP_DIV] = STRING("DIV"),
                [OP_BUILTIN] = STRING("BUILTIN")};

Str VALUE_TYPE_MAP[] = {
    [V_OPTION] = STRING("Option("), [V_STRING] = STRING("Str"),
    [V_NUM] = STRING("Number"),     [V_TRUE] = STRING("True"),
    [V_FALSE] = STRING("False"),    [V_LIST] = STRING("List"),
};

#define VM_ERR(msg)                                                            \
  fprintf(stderr, "[VM] ERROR: `" msg "` failed at %s, line %d\n", __FILE__,   \
          __LINE__);                                                           \
  goto vm_end;

void Value_debug(Value *v) {
  Str_debug(&VALUE_TYPE_MAP[v->type]);
  switch (v->type) {
  case V_OPTION: {
    if (v->option.is_some) {
      printf("Some(");
      Value_debug(v->option.value);
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
    Str_debug(&v->string);
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
}

int Vm_run(Vm *vm) {
#if DEBUG
  puts("================= GLOB =================");
  for (size_t i = 0; i < vm->global_len; i++) {
    printf("VM[glob%zu/%zu] ", i + 1, vm->global_len);
    Value_debug(&vm->globals[i]);
    puts("");
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
    case OP_ADD: {
      Value *a = &vm->registers[0];
      Value *b = &vm->registers[arg];
      if (a->type != b->type) {
        VM_ERR("VM[+] Incompatible type")
      }
      switch (a->type) {
      case V_NUM:
        vm->registers[0] = (Value){.type = V_NUM,
                                   .number = vm->registers[0].number +
                                             vm->registers[arg].number};
        break;
      case V_STRING:
        VM_ERR("VM[+] Str concat not implemented yet")
      default:
        VM_ERR("VM[+] Only strings and numbers can be concatinated")
      }
      break;
    }
    case OP_SUB: {
      Value *a = &vm->registers[0];
      Value *b = &vm->registers[arg];
      if (a->type != V_NUM || b->type != V_NUM) {
        VM_ERR("VM[-] Subtraction is only allowed for numbers")
      }
      vm->registers[0] =
          (Value){.type = V_NUM, .number = b->number - a->number};
      break;
    }
    case OP_MUL: {
      Value *a = &vm->registers[0];
      Value *b = &vm->registers[arg];
      if (a->type != V_NUM || b->type != V_NUM) {
        VM_ERR("VM[*] Multiplication is only allowed for numbers")
      }
      vm->registers[0] =
          (Value){.type = V_NUM, .number = b->number * a->number};
      break;
    }
    case OP_DIV: {
      Value *a = &vm->registers[0];
      Value *b = &vm->registers[arg];
      if (a->type != V_NUM || b->type != V_NUM) {
        VM_ERR("VM[/] Division is only allowed for numbers")
      }
      vm->registers[0] =
          (Value){.type = V_NUM, .number = b->number / a->number};
      break;
    }
    case OP_BUILTIN:
      // TODO: more checks here if we would handle some builtins differently
      // from just calling a function
      vm->registers[0] = BUILTIN_MAP[arg](vm->registers[0]);
      break;
    default:
      VM_ERR("Unimplemented instruction")
    }
    vm->pc += 2;
  }
#if DEBUG
  puts("================= REGS =================");
#define REGISTER_PRINT_COUNT 3
  for (size_t i = 0; i < REGISTER_PRINT_COUNT; i++) {
    printf("VM[r%zu]: ", i);
    Value_debug(&vm->registers[i]);
    puts("");
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
