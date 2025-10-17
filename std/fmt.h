#include "../vm.h"

static void print_value(const Value *v) {
  if (v->is_some) {
    printf("Option::Some(");
  }
  switch (v->type) {
  case V_NONE:
    printf("Option::None");
    break;
  case V_STR:
    Str_debug(v->string);
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
  case V_OBJ:
    // TODO: V_OBJ
    printf("{}");
    break;
  case V_ARRAY:
    printf("[");
    uint64_t len = v->array->len;
    for (size_t i = 0; i < len; i++) {
      Value e = LIST_get(v->array, i);
      print_value(&e);
      if (i + 1 < len) {
        printf(", ");
      }
    }
    printf("]");
    break;
  default:
  }

  if (v->is_some) {
    printf(")");
  }
}

// print works the same as println but without the newline
void builtin_fmt_print(Vm *vm) {
  for (uint16_t i = 0; i < vm->arg_count; i++) {
    print_value(&ARG(i));
    putc(' ', stdout);
  }
  RETURN(*INTERNED_NONE);
}

// println outputs its argument to stdout, joined with ' ' and postfixed with a
// newline
void builtin_fmt_println(Vm *vm) {
  builtin_fmt_print(vm);
  putc('\n', stdout);
  RETURN(*INTERNED_NONE);
}
