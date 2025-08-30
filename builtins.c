#include "builtins.h"
#include "common.h"

static void print_value(const Value *v) {
  if (v->is_some) {
    printf("Option/Some(");
  }
  switch (v->type) {
  case V_NONE:
    printf("Option/None");
    break;
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
  case V_OBJ:
    // TODO: V_OBJ
    printf("{}");
    break;
  case V_ARRAY:
    printf("[");
    for (size_t i = 0; i < v->array.len; i++) {
      print_value(&((Value *)v->array.elements)[i]);
      if (i + 1 < v->array.len) {
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
void builtin_print(Vm *vm) {
  for (uint16_t i = 0; i < vm->arg_count; i++) {
    print_value(&ARG(i));
    putc(' ', stdout);
  }
  RETURN(*INTERNED_NONE);
}

// println outputs its argument to stdout, joined with ' ' and postfixed with a
// newline
void builtin_println(Vm *vm) {
  builtin_print(vm);
  putc('\n', stdout);
  RETURN(*INTERNED_NONE);
}

void builtin_len(Vm *vm) {
  ASSERT(vm->arg_count == 1, "@len only works for a singular argument")
  const Value *a = &ARG(0);
  size_t len = 0;
  if (a->type == V_STR) {
    len = a->string.len;
  } else if (a->type == V_ARRAY) {
    len = a->array.len;
  } else if (a->type == V_OBJ) {
    len = a->obj.size;
  } else {
    fputs("@len only strings and lists have a length", stderr);
    exit(EXIT_FAILURE);
  }

  RETURN({
      .type = V_INT,
      .integer = len,
  });
}

void builtin_type(Vm *vm) {
  ASSERT(vm->arg_count == 1, "@type only works for a singular argument")
  uint16_t offset = vm->arg_offset;
  Str s;
  const Value *a = &ARG(0);
  if (a->is_some) {
    s = STRING("option");
  } else {
    switch (a->type) {
    case V_NONE:
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
    case V_OBJ:
      s = STRING("object");
      break;
    default:
      fputs("@type internal error: unknown value type", stderr);
      exit(EXIT_FAILURE);
      break;
    }
  }

  RETURN({
      .type = V_STR,
      .string = s,
  });
}

void builtin_Some(Vm *vm) {
  ASSERT(vm->arg_count == 1, "@Some only works for a singular argument")
  Value inner = ARG(0);
  inner.is_some = true;
  RETURN(inner);
}
