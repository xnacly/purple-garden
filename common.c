#include "common.h"
#include <stdlib.h>
#include <string.h>

char String_get(String *str, size_t index) {
  if (index >= str->len - 1) {
    return -1;
  }
  return (unsigned int)str->p[index];
}

char *String_to(String *str) {
  size_t len = str->len;
  char *s = malloc((len + 1) * sizeof(char *));
  memcpy(s, str->p, len);
  s[len + 1] = '\0';
  return s;
}

String String_from(char *s) {
  return (String){
      .len = strlen(s),
      .p = s,
  };
}

String String_slice(String *str, size_t start, size_t end) {
  ASSERT(end >= start,
         "String_slice: Invalid slice range: end must be >= start");
  ASSERT(end <= str->len, "String_slice: Slice range exceeds string length");

  return (String){
      .p = str->p + start,
      .len = end - start,
  };
}

bool String_eq(String *a, String *b) {
  ASSERT(a != NULL, "String_eq: a is NULL");
  ASSERT(b != NULL, "String_eq: b is NULL");
  if (a->len != b->len) {
    return false;
  }

  return 0 == memcmp(a->p, b->p, a->len);
}

void String_debug(String *str) { printf("%.*s", (int)str->len, str->p); }

#define PREC 1e7

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
  case V_STRING:
    return String_eq(&a.string, &b.string);
  case V_NUM:
    // comparing doubles by subtracting them and comparing the result to an
    // epsilon is okay id say
    return _fabs(a.number - b.number) < PREC;
  case V_TRUE:
  case V_FALSE:
    return true;
  case V_LIST:
  default:
    // lists arent really the same, this is not a deep equal
    return false;
  }
}

#undef PREC
