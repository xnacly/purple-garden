#include "strings.h"
#include "common.h"
#include <string.h>

char Str_get(const Str *str, size_t index) {
  if (str == NULL || index >= str->len - 1) {
    return -1;
  }
  return (unsigned int)str->p[index];
}

Str Str_from(const char *s) {
  return (Str){
      .len = strlen(s),
      .p = s,
  };
}

Str Str_slice(const Str *str, size_t start, size_t end) {
  ASSERT(end >= start, "Str_slice: Invalid slice range: end must be >= start");
  ASSERT(end <= str->len, "Str_slice: Slice range exceeds string length");

  return (Str){
      .p = str->p + start,
      .len = end - start,
  };
}

bool Str_eq(const Str *a, const Str *b) {
  ASSERT(a != NULL, "Str_eq: a is NULL");
  ASSERT(b != NULL, "Str_eq: b is NULL");
  if (a->len != b->len) {
    return false;
  }

  return 0 == memcmp(a->p, b->p, a->len);
}

void Str_debug(const Str *str) { printf("%.*s", (int)str->len, str->p); }

inline size_t Str_hash(const Str *str) {
  // https://en.wikipedia.org/wiki/Fowler%E2%80%93Noll%E2%80%93Vo_hash_function#FNV-1a_hash
  // https://en.wikipedia.org/wiki/Fowler%E2%80%93Noll%E2%80%93Vo_hash_function#FNV_hash_parameters
  size_t hash = FNV_OFFSET_BASIS;
  for (size_t i = 0; i < str->len; i++) {
    hash ^= str->p[i];
    hash *= FNV_PRIME;
  }

  return hash & GLOBAL_MASK;
}

int64_t Str_to_int64_t(const Str *str) {
  int64_t r = 0;
  ASSERT(str->len > 0, "Cant convert empty string into int64_t");

  for (size_t i = 0; i < str->len; i++) {
    int digit = str->p[i] - '0';
    ASSERT(r < (INT64_MAX - digit) / 10,
           "int64_t number space overflow: `%.*s`", (int)str->len, str->p)
    r = r * 10 + digit;
  }

  return r;
}

double Str_to_double(const Str *str) {
  ASSERT(str->len > 0, "Can't convert empty string into double");

  const char *p = str->p;
  size_t len = str->len;

  uint64_t mantissa = 0;
  int exponent = 0; // decimal exponent
  bool seen_dot = false;
  bool has_digits = false;

  for (size_t i = 0; i < len; i++) {
    char c = p[i];

    if (c >= '0' && c <= '9') {
      has_digits = true;
      mantissa = mantissa * 10 + (c - '0');
      if (seen_dot) {
        exponent -= 1;
      }
    } else if (c == '.') {
      ASSERT(!seen_dot, "Multiple dots in double: `%.*s`", (int)len, p);
      seen_dot = true;
    } else {
      ASSERT(false,
             "Invalid character in double `%.*s`, stopped at position %zu",
             (int)len, p, i);
    }
  }

  ASSERT(has_digits, "Can't parse `%.*s` into a double", (int)len, p);

  // Scale mantissa by 10^exponent
  double result = (double)mantissa;
  if (exponent < 0) {
    for (int i = 0; i < -exponent; i++)
      result /= 10.0;
  } else {
    for (int i = 0; i < exponent; i++)
      result *= 10.0;
  }

  return result;
}
