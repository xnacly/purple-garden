#include "../vm.h"

static void pg_builtin_opt_some(Vm *vm) {
  Value inner = ARG(0);
  inner.is_some = true;
  RETURN(inner);
}

static void pg_builtin_opt_none(Vm *vm) { RETURN(*INTERNED_NONE); }

static void pg_builtin_opt_or(Vm *vm) {
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

static void pg_builtin_opt_unwrap(Vm *vm) {
  Value v = ARG(0);
  ASSERT(v.type != V_NONE, "Unwrap: called on a Option::None value");
  ASSERT(v.is_some, "Unwrap: Attempted to unwrap a non optional value");
  v.is_some = false;
  RETURN(v);
}

static void pg_builtin_opt_expect(Vm *vm) {
  Value v = ARG(0);
  Value msg = ARG(1);
  ASSERT(v.type != V_NONE, "Expect: %.*s", (int)msg.string.len, msg.string.p);
  ASSERT(v.is_some, "Expect: Attempted to expect a non optional value");
  v.is_some = false;
  RETURN(v);
}

static void pg_builtin_opt_is_some(Vm *vm) {
  Value v = ARG(0);
  RETURN({.type = ARG(0).is_some ? V_TRUE : V_FALSE});
}

static void pg_builtin_opt_is_none(Vm *vm) {
  RETURN({.type = ARG(0).type == V_NONE ? V_TRUE : V_FALSE});
}
