#include "builtins.h"
#include "common.h"

static void print_value(const Value *v) {
  switch (v->type) {
  case V_UNDEFINED:
    printf("undefined");
    break;
  case V_OPTION: {
    if (v->option.is_some) {
      printf("Some(");
      print_value(v->option.value);
      putc(')', stdout);
    } else {
      printf("None");
    }
    break;
  }
  case V_STR:
    Str_debug(&v->string);
    break;
  case V_DOUBLE:
    printf("%g", v->floating);
    break;
  case V_INT:
    printf("%ld", v->integer);
    break;
  case V_TRUE:
    printf("true");
    break;
  case V_FALSE:
    printf("false");
    break;
  case V_ARRAY:
    printf("[");
    for (size_t i = 0; i < v->array.len; i++) {
      print_value(v->array.value[i]);
      if (i + 1 < v->array.len) {
        printf(", ");
      }
    }
    printf("]");
    break;
  default:
  }
}

// print works the same as println but without the newline
void builtin_print(Vm *vm) {
  for (size_t i = 1; i < vm->arg_count + 1; i++) {
    print_value(&vm->registers[i]);
    putc(' ', stdout);
  }
}

// println outputs its argument to stdout, joined with ' ' and postfixed with a
// newline
void builtin_println(Vm *vm) {
  builtin_print(vm);
  putc('\n', stdout);
}

// len returns the value of its argument:
//
// - for V_STRING: string length
// - for V_LIST: amount of children in list
// - else None
void builtin_len(Vm *vm) {
  ASSERT(vm->arg_count == 1, "@len only works for a singular argument")
  const Value *a = &vm->registers[1];
  size_t len = 0;
  if (a->type == V_STR) {
    len = a->string.len;
  } else if (a->type == V_ARRAY) {
    len = a->array.len;
  } else {
    fputs("@len only strings and lists have a length", stderr);
    exit(EXIT_FAILURE);
  }

  vm->registers[0] = (Value){
      .type = V_INT,
      .integer = len,
  };
}

void builtin_type(Vm *vm) {
  ASSERT(vm->arg_count == 1, "@type only works for a singular argument")
  Str s;
  switch (vm->registers[1].type) {
  case V_OPTION:
    s = STRING("option");
    break;
  case V_STR:
    s = STRING("string");
    break;
  case V_INT:
  case V_DOUBLE:
    s = STRING("number");
    break;
  case V_TRUE:
  case V_FALSE:
    s = STRING("boolean");
    break;
  case V_ARRAY:
    s = STRING("array");
    break;
  default:
  }

  vm->registers[0] = (Value){
      .type = V_STR,
      .string = s,
  };
}

void builtin_Some(Vm *vm) {
  ASSERT(vm->arg_count == 1, "@type only works for a singular argument")
  Value *inner = vm->alloc->request(vm->alloc->ctx, sizeof(Value));
  *inner = vm->registers[1];
  struct Option o = (struct Option){
      .is_some = true,
      .value = inner,
  };
  vm->registers[0] = (Value){
      .type = V_OPTION,
      .option = o,
  };
}
