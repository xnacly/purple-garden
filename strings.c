#include "strings.h"
#include "common.h"
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
