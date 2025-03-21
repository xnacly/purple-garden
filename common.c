#include "common.h"
#include <stdlib.h>
#include <string.h>

char String_get(String *str, size_t index) {
  if (index >= str->len - 1) {
    return -1;
  }
  return (unsigned int)str->p[index];
}

char *String_to(String *str) { return str->p; }

String String_from(char *s) {
  return (String){
      .len = strlen(s),
      .p = s,
  };
}

String String_slice(String *str, size_t start, size_t end) {
  size_t size = end - start;
  char *s = malloc(size + 1);
  strncpy(s, str->p + start, size);
  s[size] = '\0';
  return (String){
      .len = size + 1,
      .p = s,
  };
}

boolean String_eq(String *a, String *b) {
  ASSERT(a != NULL, "String_eq#a is NULL");
  ASSERT(b != NULL, "String_eq#b is NULL");
  if (a->len != b->len) {
    return false;
  }

  return 0 == memcmp(a->p, b->p, a->len);
}
