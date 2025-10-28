#include "../vm.h"
#include <math.h>

static void builtin_math_mod(Vm *vm) {
  Value *lhs = &ARG(0);
  Value *rhs = &ARG(1);

  int lhs_is_double = lhs->type == V_DOUBLE;
  int rhs_is_double = rhs->type == V_DOUBLE;

  if (rhs->integer == 0) {
    ASSERT(0, "std::math::mod(): division by zero");
  }

  if (lhs_is_double || rhs_is_double) {
    double a = lhs_is_double ? lhs->floating : (double)lhs->integer;
    double b = rhs_is_double ? rhs->floating : (double)rhs->integer;
    RETURN((Value){.type = V_DOUBLE, .floating = fmod(a, b)});
    return;
  }

  if (lhs->type == V_INT && rhs->type == V_INT) {
    RETURN((Value){.type = V_INT, .integer = lhs->integer % rhs->integer});
    return;
  }

  Str l = VALUE_TYPE_MAP[lhs->type];
  Str r = VALUE_TYPE_MAP[rhs->type];
  ASSERT(0, "std::math::mod(): incompatible types `%.*s` and `%.*s`",
         (int)l.len, l.p, (int)r.len, r.p);
}
