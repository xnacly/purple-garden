#include "common.h"

#define PREC 1e-7

static double _fabs(double x) { return x < 0 ? -x : x; }

// Value_cmp compares two values in a shallow way, is used for OP_EQ and in
// tests.
//
// Edgecase: V_LIST & V_LIST is false, because we do a shallow compare
bool Value_cmp(Value a, Value b) {
  if (a.type != b.type) {
    return false;
  }

  switch (a.type) {
  case V_STR:
    return Str_eq(&a.string, &b.string);
  case V_NUM:
    // comparing doubles by subtracting them and comparing the result to an
    // epsilon is okay id say
    return _fabs(a.number - b.number) < PREC;
  case V_TRUE:
  case V_FALSE:
    return true;
  case V_ARRAY:
  default:
    // lists arent really the same, this is not a deep equal
    return false;
  }
}

#undef PREC
