#include "builtins.h"
#include "common.h"

static void print_value(const Value v) {
  switch (v.type) {
  case V_OPTION: {
    if (v.option.is_some) {
      printf("Some(");
      print_value(*v.option.value);
      putc(')', stdout);
    } else {
      printf("None");
    }
    break;
  }
  case V_STR:
    Str_debug(&v.string);
    break;
  case V_DOUBLE:
    printf("%g", v.floating);
    break;
  case V_INT:
    printf("%ld", v.integer);
    break;
  case V_TRUE:
    printf("true");
    break;
  case V_FALSE:
    printf("false");
    break;
  case V_ARRAY:
    printf("[");
    for (size_t i = 0; i < v.array.len; i++) {
      print_value(v.array.value[i]);
      if (i + 1 <= v.array.len) {
        printf(", ");
      }
    }
    printf("]");
    break;
  case V_UNDEFINED:
    printf("undefined");
    break;
  default:
  }
}

// print works the same as println but without the newline
Value builtin_print(const Value *arg, size_t count) {
  if (count == 1) {
    print_value(arg[0]);
  } else {
    for (size_t i = 0; i < count; i++) {
      print_value(arg[i]);
      putc(' ', stdout);
    }
  }
  return NONE;
}

// println outputs its argument to stdout, joined with ' ' and postfixed with a
// newline
Value builtin_println(const Value *arg, size_t count) {
  builtin_print(arg, count);
  putc('\n', stdout);
  return NONE;
}

// len returns the value of its argument:
//
// - for V_STRING: string length
// - for V_LIST: amount of children in list
// - else None
Value builtin_len(const Value *arg, size_t count) {
  ASSERT(count == 1, "len only works for a singular argument")
  const Value *a = &arg[0];
  if (a->type == V_STR) {
    return (Value){.type = V_INT, .integer = a->string.len};
  } else if (a->type == V_ARRAY) {
    return (Value){.type = V_INT, .integer = a->array.len};
  } else {
    fputs("builtin_len only strings and lists have a length", stderr);
    exit(EXIT_FAILURE);
  }
  return NONE;
}

Value builtin_type(const Value *arg, size_t count) {
  ASSERT(count == 1, "type only accepts one argument")
  Str s;
  switch (arg->type) {
  case V_UNDEFINED:
    s = STRING("undefined");
    break;
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
  return (Value){.type = V_STR, .string = s};
}

Value builtin_assert(const Value *arg, size_t count) {
  ASSERT(count == 2, "@assert: can only compare 2 arguments, got %zu", count);
  const Value *lhs = &arg[0];
  const Value *rhs = &arg[1];
  if (!Value_cmp(lhs, rhs)) {
    printf("@assert: ");
    Value_debug(lhs);
    printf(" != ");
    Value_debug(rhs);
    puts("");
    ASSERT(0, "Assertion failed");
  }
  return NONE;
}

builtin_function BUILTIN_MAP[] = {
    [BUILTIN_ASSERT] = &builtin_assert, [BUILTIN_PRINTLN] = &builtin_println,
    [BUILTIN_PRINT] = &builtin_print,   [BUILTIN_TYPE] = &builtin_type,
    [BUILTIN_LEN] = &builtin_len,
};

Str BUILTIN_NAME_MAP[] = {
    [BUILTIN_ASSERT] = STRING("assert"), [BUILTIN_PRINTLN] = STRING("println"),
    [BUILTIN_PRINT] = STRING("print"),   [BUILTIN_TYPE] = STRING("type"),
    [BUILTIN_LEN] = STRING("len"),
};
