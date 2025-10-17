#include "../vm.h"

static const Str *OPTION = &STRING("option");
static const Str *STR = &STRING("str");
static const Str *NUM = &STRING("number");
static const Str *BOOL = &STRING("bool");
static const Str *OBJ = &STRING("obj");
static const Str *ARR = &STRING("array");

static void builtin_runtime_type(Vm *vm) {
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

static void builtin_runtime_assert(Vm *vm) {
  ASSERT(vm->arg_count == 1, "assert needs exactly a single argument, got %d",
         vm->arg_count);
  Value v = ARG(0);
  ASSERT(v.type = V_TRUE, "Assertion failed");
}

static void builtin_runtime_gc_stats(Vm *vm) {
  // TODO: this fucking sucks, there has to be a better way, i urgenly need a
  // c->pg type macro
  Stats c = CALL(vm->alloc, stats);
  LIST_Value *lv = CALL(vm->alloc, request, sizeof(LIST_Value));
  *lv = LIST_new(Value);
  Value current = (Value){.type = V_INT, .integer = c.current};
  Value allocated = (Value){.type = V_INT, .integer = c.allocated};
  LIST_append(lv, vm->alloc, allocated);
  LIST_append(lv, vm->alloc, current);

  RETURN((Value){
      .type = V_ARRAY,
      .array = lv,
  });
}
