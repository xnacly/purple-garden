#include "strings.h"
#include "common.h"
#include <string.h>

char Str_get(Str *str, size_t index) {
  if (index >= str->len - 1) {
    return -1;
  }
  return (unsigned int)str->p[index];
}

char *Str_to(Str *str) {
  size_t len = str->len;
  char *s = malloc((len + 1) * sizeof(char *));
  memcpy(s, str->p, len);
  s[len + 1] = '\0';
  return s;
}

Str Str_from(char *s) {
  return (Str){
      .len = strlen(s),
      .p = s,
  };
}

Str Str_slice(Str *str, size_t start, size_t end) {
  ASSERT(end >= start, "Str_slice: Invalid slice range: end must be >= start");
  ASSERT(end <= str->len, "Str_slice: Slice range exceeds string length");

  return (Str){
      .p = str->p + start,
      .len = end - start,
  };
}

bool Str_eq(Str *a, Str *b) {
  ASSERT(a != NULL, "Str_eq: a is NULL");
  ASSERT(b != NULL, "Str_eq: b is NULL");
  if (a->len != b->len) {
    return false;
  }

  return 0 == memcmp(a->p, b->p, a->len);
}

void Str_debug(Str *str) { printf("%.*s", (int)str->len, str->p); }
