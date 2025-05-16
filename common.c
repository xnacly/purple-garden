#include "common.h"

#define PREC 1e-7

Str VALUE_TYPE_MAP[] = {
    [V_OPTION] = STRING("Option("), [V_STR] = STRING("Str"),
    [V_INT] = STRING("Int"),        [V_DOUBLE] = STRING("Double"),
    [V_TRUE] = STRING("True"),      [V_FALSE] = STRING("False"),
    [V_ARRAY] = STRING("Array"),
};

static double _fabs(double x) { return x < 0 ? -x : x; }

// Value_cmp compares two values in a shallow way, is used for OP_EQ and in
// tests.
//
// Edgecases:
// - V_LIST & V_LIST is false, because we do a shallow compare
// - V_OPTION(Some(A)) & V_OPTION(Some(B)) even with matching A and B is false,
// since we do not compare inner
bool Value_cmp(const Value *a, const Value *b) {
  if (a->type != b->type) {
    return false;
  }

  switch (a->type) {
  case V_STR:
    return Str_eq(&a->string, &b->string);
  case V_DOUBLE:
    // comparing doubles by subtracting them and comparing the result to an
    // epsilon is okay id say
    return _fabs(a->floating - b->floating) < PREC;
  case V_INT:
    return a->integer == b->integer;
  case V_TRUE:
  case V_FALSE:
    return true;
  case V_OPTION:
    return !(a->option.is_some || b->option.is_some);
  case V_ARRAY:
  default:
    // lists arent really the same, this is not a deep equal
    return false;
  }
}

void Value_debug(const Value *v) {
  Str_debug(&VALUE_TYPE_MAP[v->type]);
  switch (v->type) {
  case V_OPTION: {
    if (v->option.is_some) {
      printf("Some(");
      Value_debug(v->option.value);
      printf(")");
    } else {
      printf("None");
    }
    putc(')', stdout);
    break;
  }
  case V_TRUE:
  case V_FALSE:
    break;
  case V_STR:
    printf("(`");
    Str_debug(&v->string);
    printf("`)");
    break;
  case V_DOUBLE:
    printf("(%g)", v->floating);
    break;
  case V_INT:
    printf("(%ld)", v->integer);
    break;
  case V_UNDEFINED:
    printf("undefined");
    break;
  case V_ARRAY: {
    printf("[");
    for (size_t i = 0; i < v->array.len; i++) {
      Value_debug(&v->array.value[i]);
    }
    printf("]");
    break;
  };
  default:
    printf("<unkown>");
  }
}

#undef PREC
