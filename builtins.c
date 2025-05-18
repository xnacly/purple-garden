#include "builtins.h"
#include "common.h"

static Value *NONE =
    &(Value){.type = V_OPTION, .option = (struct Option){.is_some = false}};

static void print_value(const Value *v) {
  switch (v->type) {
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
Value *builtin_print(const Value **arg, size_t count, Allocator *alloc) {
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
Value *builtin_println(const Value **arg, size_t count, Allocator *alloc) {
  builtin_print(arg, count, alloc);
  putc('\n', stdout);
  return NONE;
}

// len returns the value of its argument:
//
// - for V_STRING: string length
// - for V_LIST: amount of children in list
// - else None
Value *builtin_len(const Value **arg, size_t count, Allocator *alloc) {
  ASSERT(count == 1, "len only works for a singular argument")
  const Value *a = arg[0];
  if (a->type == V_STR) {
    Value *v = alloc->request(alloc->ctx, sizeof(Value));
    v->type = V_INT;
    v->integer = a->string.len;
    return v;
  } else if (a->type == V_ARRAY) {
    Value *v = alloc->request(alloc->ctx, sizeof(Value));
    v->type = V_INT;
    v->integer = a->array.len;
    return v;
  } else {
    fputs("builtin_len only strings and lists have a length", stderr);
    exit(EXIT_FAILURE);
  }
}

Value *builtin_type(const Value **arg, size_t count, Allocator *alloc) {
  ASSERT(count == 1, "type only accepts one argument")
  Str s;
  switch (arg[0]->type) {
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

  Value *v = alloc->request(alloc->ctx, sizeof(Value));
  v->type = V_STR;
  v->string = s;
  return v;
}

Value *builtin_assert(const Value **arg, size_t count, Allocator *alloc) {
  ASSERT(count == 1, "@assert: can only assert 1 argument to true, got %zu",
         count);
  ASSERT(arg[0]->type == V_TRUE,
         "@assert: assertion failed, value was not true");
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
