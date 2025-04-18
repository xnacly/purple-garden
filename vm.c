#include "vm.h"
#include "builtins.h"
#include "common.h"
#include "mem.h"
#include "strings.h"

Str OP_MAP[256] = {
    [OP_LOAD] = STRING("LOAD"),      [OP_STORE] = STRING("STORE"),
    [OP_ADD] = STRING("ADD"),        [OP_SUB] = STRING("SUB"),
    [OP_MUL] = STRING("MUL"),        [OP_DIV] = STRING("DIV"),
    [OP_PUSH] = STRING("PUSH"),      [OP_VAR] = STRING("VAR"),
    [OP_LOADV] = STRING("LOADV"),    [OP_ARGS] = STRING("ARGS"),
    [OP_BUILTIN] = STRING("BUILTIN")};

Str VALUE_TYPE_MAP[] = {
    [V_OPTION] = STRING("Option("), [V_STRING] = STRING("Str"),
    [V_NUM] = STRING("Number"),     [V_TRUE] = STRING("True"),
    [V_FALSE] = STRING("False"),    [V_LIST] = STRING("List"),
};

#define VM_ERR(fmt, ...)                                                       \
  fprintf(stderr, "[VM] ERROR: " fmt "\n", ##__VA_ARGS__);                     \
  goto vm_end;

void Value_debug(const Value *v) {
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

int Vm_run(Vm *vm, Allocator *alloc) {
  vm->arg_count = 1;
#if DEBUG
  puts("================== GLOBAL ==================");
  for (size_t i = 0; i < vm->global_len; i++) {
    printf("VM[glob%zu/%zu] ", i + 1, (size_t)vm->global_len);
    Value_debug(&vm->globals[i]);
    puts("");
  }
  puts("================== VM OPS ==================");
#endif
  while (vm->pc < vm->bytecode_len) {
    VM_OP op = vm->bytecode[vm->pc];
    uint64_t arg = vm->bytecode[vm->pc + 1];

    switch (op) {
    case OP_LOAD:
      vm->registers[0] = vm->globals[arg];
      break;
    case OP_LOADV: {
      Value v = vm->frame.variable_table[arg & VARIABLE_TABLE_SIZE];
      if (v.type == V_UNDEFINED) {
        Value *var = &vm->globals[arg & GLOBAL_MASK];
        VM_ERR("Undefined variable `%.*s` in current scope",
               (int)var->string.len, var->string.p);
      }
      vm->registers[0] = v;
      break;
    }
    case OP_STORE:
      vm->registers[arg] = vm->registers[0];
      break;
    case OP_VAR:
      vm->frame
          .variable_table[vm->registers[0].string.hash & VARIABLE_TABLE_SIZE] =
          vm->registers[arg];
      break;
    case OP_ADD: {
      Value *a = &vm->registers[0];
      Value *b = &vm->registers[arg];
      if (a->type != b->type) {
        VM_ERR("VM[+] Incompatible types %.*s and %.*s",
               (int)VALUE_TYPE_MAP[a->type].len, VALUE_TYPE_MAP[a->type].p,
               (int)VALUE_TYPE_MAP[b->type].len, VALUE_TYPE_MAP[b->type].p)
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
        VM_ERR("VM[+] Only strings and numbers can be concatenated")
      }
      break;
    }
    case OP_SUB: {
      Value *a = &vm->registers[0];
      Value *b = &vm->registers[arg];
      if (a->type != V_NUM || b->type != V_NUM) {
        VM_ERR("VM[-] Subtraction is only allowed for numbers, not for types "
               "%.*s and %.*s",
               (int)VALUE_TYPE_MAP[a->type].len, VALUE_TYPE_MAP[a->type].p,
               (int)VALUE_TYPE_MAP[b->type].len, VALUE_TYPE_MAP[b->type].p)
      }
      vm->registers[0] =
          (Value){.type = V_NUM, .number = b->number - a->number};
      break;
    }
    case OP_MUL: {
      Value *a = &vm->registers[0];
      Value *b = &vm->registers[arg];
      if (a->type != V_NUM || b->type != V_NUM) {
        VM_ERR(
            "VM[*] Multiplication is only allowed for numbers, not for types "
            "%.*s and %.*s",
            (int)VALUE_TYPE_MAP[a->type].len, VALUE_TYPE_MAP[a->type].p,
            (int)VALUE_TYPE_MAP[b->type].len, VALUE_TYPE_MAP[b->type].p)
      }
      vm->registers[0] =
          (Value){.type = V_NUM, .number = b->number * a->number};
      break;
    }
    case OP_DIV: {
      Value *a = &vm->registers[0];
      Value *b = &vm->registers[arg];
      if (a->type != V_NUM || b->type != V_NUM) {
        VM_ERR("VM[/] Subtraction is only allowed for numbers, not for types "
               "%.*s and %.*s",
               (int)VALUE_TYPE_MAP[a->type].len, VALUE_TYPE_MAP[a->type].p,
               (int)VALUE_TYPE_MAP[b->type].len, VALUE_TYPE_MAP[b->type].p)
      }
      vm->registers[0] =
          (Value){.type = V_NUM, .number = b->number / a->number};
      break;
    }
    case OP_ARGS:
      vm->arg_count = arg;
      break;
    case OP_PUSH:
      ASSERT(vm->stack_cur < CALL_ARGUMENT_STACK,
             "Out of argument stack space: %d", CALL_ARGUMENT_STACK)
      vm->stack[vm->stack_cur++] = vm->registers[0];
      break;
    case OP_BUILTIN: {
      // at this point all builtins are just syscalls into an array of function
      // pointers
      if (vm->arg_count == 1) {
        vm->registers[0] = BUILTIN_MAP[arg](&vm->registers[0], 1);
      } else {
        Value v[vm->arg_count];
        for (int i = vm->arg_count - 1; i > 0; i--) {
          ASSERT(vm->stack_cur != 0,
                 "No element in argument stack, failed to pop");
          v[i - 1] = vm->stack[--vm->stack_cur];
        }
        v[vm->arg_count - 1] = vm->registers[0];
        vm->registers[0] = BUILTIN_MAP[arg](v, vm->arg_count);
      }

      vm->arg_count = 1;
      break;
    }
    default:
      VM_ERR("Unimplemented instruction %.*s", (int)OP_MAP[op].len,
             OP_MAP[op].p)
    }
#if DEBUG
    DIS(op, arg)
#endif
    vm->pc += 2;
  }
#if DEBUG
  puts("==================  REGS  ==================");
#define REGISTER_PRINT_COUNT 3
  for (size_t i = 0; i < REGISTER_PRINT_COUNT; i++) {
    printf("VM[r%zu]: ", i);
    Value_debug(&vm->registers[i]);
    puts("");
  }
#endif
  return 0;
vm_end:
  DIS(vm->bytecode[vm->pc], (size_t)vm->bytecode[vm->pc + 1]);
  return 1;
}

void Vm_destroy(Vm vm) {}
