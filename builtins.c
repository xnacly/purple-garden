#include "builtins.h"
#include "common.h"
#include "vm.h"

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
  case V_STRING:
    Str_debug(&v.string);
    break;
  case V_NUM:
    printf("%g", v.number);
    break;
  case V_TRUE:
    printf("true");
    break;
  case V_FALSE:
    printf("false");
    break;
  case V_LIST:
    // TODO: iterate each one and print with ,
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
  if (a->type != V_STRING) {
    Value_debug(a);
    ASSERT(a->type == V_STRING, "len only works for strings")
  }
  return (Value){.type = V_NUM, .number = a->string.len};
  // if (a.type == V_STRING) {
  // } else if (a.type == V_LIST) {
  //   TODO("builtin_len#arg->type == V_LIST not implemented")
  // } else {
  //   fputs("builtin_len only strings and lists have a length", stderr);
  //   exit(EXIT_FAILURE);
  // }
  // return NONE;
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
  case V_STRING:
    s = STRING("string");
    break;
  case V_NUM:
    s = STRING("number");
    break;
  case V_TRUE:
  case V_FALSE:
    s = STRING("boolean");
    break;
  case V_LIST:
    s = STRING("list");
    break;
  default:
  }
  return (Value){.type = V_STRING, .string = s};
}

builtin_function BUILTIN_MAP[] = {
    [BUILTIN_PRINTLN] = &builtin_println,
    [BUILTIN_PRINT] = &builtin_print,
    [BUILTIN_LEN] = &builtin_len,
    [BUILTIN_TYPE] = &builtin_type,
};

Str BUILTIN_NAME_MAP[] = {
    [BUILTIN_PRINTLN] = STRING("println"),
    [BUILTIN_PRINT] = STRING("print"),
    [BUILTIN_LEN] = STRING("len"),
    [BUILTIN_TYPE] = STRING("type"),
};
