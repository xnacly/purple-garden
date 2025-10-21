#include "../vm.h"

static void builtin_opt_some(Vm *vm) {
  ASSERT(vm->arg_count == 1, "Some only works for a singular argument")
  Value inner = ARG(0);
  inner.is_some = true;
  RETURN(inner);
}

static void builtin_opt_none(Vm *vm) { RETURN(*INTERNED_NONE); }

static void builtin_opt_or(Vm *vm) {
  ASSERT(vm->arg_count == 2, "Or: Two arguments needed");
  Value lhs = ARG(0);
  Value rhs = ARG(1);
  ASSERT(Value_is_opt(&lhs), "Or: lhs wasnt an Optional");
  if (!lhs.is_some) {
    RETURN(rhs);
  } else {
    lhs.is_some = false;
    RETURN(lhs);
  }
}

static void builtin_opt_unwrap(Vm *vm) {
  ASSERT(vm->arg_count == 1, "Unwrap: Nothing to unwrap");
  Value v = ARG(0);
  ASSERT(v.type != V_NONE, "Unwrap: called on a Option::None value");
  ASSERT(v.is_some, "Unwrap: Attempted to unwrap a non optional value");
  v.is_some = false;
  RETURN(v);
}

static void builtin_opt_expect(Vm *vm) {
  ASSERT(vm->arg_count == 2, "Expect: Two arguments needed");
  Value v = ARG(0);
  Value msg = ARG(1);
  ASSERT(msg.type == V_STR, "Expect: Message has to be a str");
  ASSERT(v.type != V_NONE, "Expect: %.*s", (int)msg.string->len, msg.string->p);
  ASSERT(v.is_some, "Expect: Attempted to expect a non optional value");
  v.is_some = false;
  RETURN(v);
}

static void builtin_opt_is_some(Vm *vm) {
  Value v = ARG(0);
  RETURN({.type = ARG(0).is_some ? V_TRUE : V_FALSE});
}

static void builtin_opt_is_none(Vm *vm) {
  RETURN({.type = ARG(0).type == V_NONE ? V_TRUE : V_FALSE});
}
