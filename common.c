#include "common.h"
#include "adts.h"
#include <stdint.h>
#include <string.h>

#define PREC 1e-9

Str VALUE_TYPE_MAP[] = {
    [V_NONE] = STRING("Option::None"), [V_STR] = STRING("str"),
    [V_INT] = STRING("int"),           [V_DOUBLE] = STRING("double"),
    [V_TRUE] = STRING("true"),         [V_FALSE] = STRING("false"),
    [V_ARRAY] = STRING("arr"),         [V_OBJ] = STRING("obj"),
};

// Value_cmp compares two values in a shallow way, is used for OP_EQ and in
// tests.
//
// Edgecases:
// - V_LIST & V_LIST is false, because we do a shallow compare
// - V_OPTION(Some(A)) & V_OPTION(Some(B)) even with matching A and B is false,
// since we do not compare inner
inline bool Value_cmp(const Value *a, const Value *b) {
  // fastpath if value pointers are equal
  if (a == b) {
    return true;
  }

  if (a->type != b->type) {
    return false;
  }

  // any is an optional, we dont compare deeply
  if (a->is_some || b->is_some) {
    return false;
  }

  switch (a->type) {
  case V_STR:
    return Str_eq(a->string, b->string);
  case V_DOUBLE:
    // PERF: can potentially be faster, since we omit need a function call, in
    // practice i havent seen any impact over the following construct. if
    //
    // (memcmp(&a->floating, &b->floating, sizeof(double)) == 0)
    //   return true;

    double diff = a->floating - b->floating;
    return (diff < PREC && diff > -PREC);
  case V_INT:
    return a->integer == b->integer;
  case V_TRUE:
  case V_FALSE:
  case V_NONE:
    return true;
  case V_ARRAY:
  default:
    // lists arent really the same, this is not a deep equal
    return false;
  }
}

void Value_debug(const Value *v) {
  if (v->is_heap) {
    printf("GC<<");
  }
  Str *t = &VALUE_TYPE_MAP[v->type];
  if (t != NULL) {
    if (v->is_some) {
      printf("Option::Some(");
    }
    Str_debug(t);
  }
  switch (v->type) {
  case V_NONE:
  case V_TRUE:
  case V_FALSE:
    break;
  case V_STR:
    printf("::\"");
    Str_debug(v->string);
    printf("\"");
    break;
  case V_DOUBLE:
    printf("::(%g)", v->floating);
    break;
  case V_INT:
    printf("::(%ld)", v->integer);
    break;
  case V_OBJ:
    // TODO: V_OBJ
    printf("::({})");
    break;
  case V_ARRAY: {
    printf("::[");
    uint64_t len = v->array->len;
    for (size_t i = 0; i < len; i++) {
      Value v_at_i = v->array->arr[i];
      Value_debug(&v_at_i);
      if (i < len - 1) {
        putc(' ', stdout);
      }
    }
    printf("]");
    break;
  };
  default:
    printf("<unkown>");
  }

  if (v->is_some) {
    printf(")");
  }

  if (v->is_heap) {
    printf(">>");
  }
}

inline double Value_as_double(const Value *v) {
  if (v->type == V_DOUBLE) {
    return v->floating;
  } else if (v->type == V_INT) {
    return (double)v->integer;
  } else {
    ASSERT(0, "Value is neither double nor int, cant convert to double")
  }
}

inline int64_t Value_as_int(const Value *v) {
  if (v->type == V_DOUBLE) {
    return (int64_t)v->floating;
  } else if (v->type == V_INT) {
    return v->integer;
  } else {
    ASSERT(0, "Value is neither double nor int, cant convert to int")
  }
}

inline bool Value_is_opt(const Value *v) {
  return v->type == V_NONE || v->is_some;
}

#undef PREC
