#include "builtins.h"
#include "adts.h"
#include "common.h"

// TODO: port this to the exception system once its implemented

static void print_value(const Value *v) {
  if (v->is_some) {
    printf("Option/Some(");
  }
  switch (v->type) {
  case V_NONE:
    printf("Option/None");
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
  ASSERT(vm->arg_count == 1, "len only works for a singular argument")
  const Value *a = &ARG(0);
  size_t len = 0;
  if (a->type == V_STR) {
    len = a->string->len;
  } else if (a->type == V_ARRAY) {
    len = a->array->len;
  } /*else if (a->type == V_OBJ) {
    len = a->obj.size;
  }*/
  else {
    fputs("len only strings and lists have a length", stderr);
    exit(EXIT_FAILURE);
  }

  RETURN({
      .type = V_INT,
      .integer = len,
  });
}

static const Str *OPTION = &STRING("option");
static const Str *STR = &STRING("str");
static const Str *NUM = &STRING("number");
static const Str *BOOL = &STRING("bool");
static const Str *OBJ = &STRING("obj");
static const Str *ARR = &STRING("array");

void builtin_type(Vm *vm) {
  ASSERT(vm->arg_count == 1, "type only works for a singular argument")
  uint16_t offset = vm->arg_offset;
  const Str *s;
  const Value *a = &ARG(0);
  if (a->is_some) {
    s = OPTION;
  } else {
    switch (a->type) {
    case V_NONE:
      s = OPTION;
      break;
    case V_STR:
      s = STR;
      break;
    case V_INT:
    case V_DOUBLE:
      s = NUM;
      break;
    case V_TRUE:
    case V_FALSE:
      s = BOOL;
      break;
    case V_ARRAY:
      s = ARR;
      break;
    case V_OBJ:
      s = OBJ;
      break;
    default:
      fputs("type internal error: unknown value type", stderr);
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
  ASSERT(vm->arg_count == 1, "Some only works for a singular argument")
  Value inner = ARG(0);
  inner.is_some = true;
  RETURN(inner);
}

void builtin_None(Vm *vm) { RETURN(*INTERNED_NONE); }

void builtin_assert(Vm *vm) {
  ASSERT(vm->arg_count == 1, "assert can't compare nothing")
  Value v = ARG(0);
  ASSERT(v.type = V_TRUE, "Assertion failed");
}
