#include "../vm.h"

static void builtin_opt_some(Vm *vm) {
  BUILTIN_CONTRACT(1);
  Value inner = ARG(0);
  inner.is_some = true;
  RETURN(inner);
}

static void builtin_opt_none(Vm *vm) { RETURN(*INTERNED_NONE); }

static void builtin_opt_or(Vm *vm) {
  BUILTIN_CONTRACT(2);
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
  BUILTIN_CONTRACT(1);
  Value v = ARG(0);
  ASSERT(v.type != V_NONE, "Unwrap: called on a Option::None value");
  ASSERT(v.is_some, "Unwrap: Attempted to unwrap a non optional value");
  v.is_some = false;
  RETURN(v);
}

static void builtin_opt_expect(Vm *vm) {
  BUILTIN_CONTRACT(2, BUILTIN_CONTRACT_ARGUMENT_TYPE(1, V_STR));
  Value v = ARG(0);
  Value msg = ARG(1);
  ASSERT(v.type != V_NONE, "Expect: %.*s", (int)msg.string->len, msg.string->p);
  ASSERT(v.is_some, "Expect: Attempted to expect a non optional value");
  v.is_some = false;
  RETURN(v);
}

static void builtin_opt_is_some(Vm *vm) {
  BUILTIN_CONTRACT(1);
  Value v = ARG(0);
  RETURN({.type = ARG(0).is_some ? V_TRUE : V_FALSE});
}

static void builtin_opt_is_none(Vm *vm) {
  BUILTIN_CONTRACT(1);
  RETURN({.type = ARG(0).type == V_NONE ? V_TRUE : V_FALSE});
}
