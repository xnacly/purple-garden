#include "../vm.h"

static void pg_builtin_str_append(Vm *vm) {
  ASSERT(vm->arg_count >= 2, "args must be at least 2");
  Value arg0 = ARG(0);
  ASSERT(arg0.type == V_STR, "arg0 must be a string");

  // Compute total length
  size_t total_len = 0;
  for (size_t i = 0; i < vm->arg_count; i++) {
    Value arg = ARG(i);
    ASSERT(arg.type == V_STR, "append: all arguments must be strings");
    total_len += arg.string.len;
  }

  uint8_t *buf = gc_request(vm->gc, total_len, GC_OBJ_RAW);

  size_t offset = 0;
  for (size_t i = 0; i < vm->arg_count; i++) {
    Value arg = ARG(i);
    memcpy(buf + offset, arg.string.p, arg.string.len);
    offset += arg.string.len;
  }

  RETURN({
      .type = V_STR,
      .is_heap = 1,
      .string = (Str){.p = buf, .len = total_len},
  });
}

static void pg_builtin_str_lines(Vm *vm) {
  Value arg = ARG(0);
  ASSERT(arg.type == V_STR, "arg0 must be a string");

  List *split = gc_request(vm->gc, sizeof(List), GC_OBJ_LIST);
  *split = List_new(8, vm->gc);

  size_t last_idx = 0;
  size_t len = arg.string.len;
  const uint8_t *p = arg.string.p;

  for (size_t i = 0; i < len; i++) {
    if (p[i] == '\n') {
      Str member = Str_slice(&arg.string, last_idx, i);
      // +1, since there are newlines
      last_idx = i + 1;
      List_append(split,
                  (Value){
                      .type = V_STR,
                      .string = member,
                  },
                  vm->gc);
    }
  }

  if (last_idx < len) {
    Str member = Str_slice(&arg.string, last_idx, len);
    List_append(split,
                (Value){
                    .type = V_STR,
                    .string = member,
                },
                vm->gc);
  }

  RETURN({
      .type = V_ARRAY,
      .array = split,
      .is_heap = true,
  });
}

static void pg_builtin_str_slice(Vm *vm) {
  Value arg0 = ARG(0);
  ASSERT(arg0.type == V_STR, "arg0 must be a string");

  Value arg1 = ARG(1);
  Value arg2 = ARG(2);

  ASSERT(((1 << arg1.type) & V_NUM_MASK) && ((1 << arg2.type) & V_NUM_MASK),
         "arg1 and arg2 must be numeric");
  int64_t start = Value_as_int(&arg1);
  int64_t end = Value_as_int(&arg2);

  ASSERT(end >= start, "str::slice: Invalid slice range: end must be >= start");
  ASSERT(end <= arg0.string.len,
         "str::slice: Slice range exceeds string length");

  RETURN({.type = V_STR, .string = Str_slice(&arg0.string, start, end)});
}
