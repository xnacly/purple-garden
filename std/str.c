#include "../vm.h"

static void builtin_str_append(Vm *vm) {
  ASSERT(vm->arg_count >= 2, "args must be at least 2");
  Value arg0 = ARG(0);
  ASSERT(arg0.type == V_STR, "arg0 must be a string");

  // Compute total length
  size_t total_len = 0;
  for (size_t i = 0; i < vm->arg_count; i++) {
    Value arg = ARG(i);
    ASSERT(arg.type == V_STR, "append: all arguments must be strings");
    total_len += arg.string->len;
  }

  Str *s = gc_request(vm->gc, sizeof(Str), GC_OBJ_STR);
  uint8_t *buf = gc_request(vm->gc, total_len, GC_OBJ_RAW);

  size_t offset = 0;
  for (size_t i = 0; i < vm->arg_count; i++) {
    Value arg = ARG(i);
    memcpy(buf + offset, arg.string->p, arg.string->len);
    offset += arg.string->len;
  }

  s->p = buf;
  s->len = total_len;

  RETURN({
      .type = V_STR,
      .is_heap = 1,
      .string = s,
  });
}
