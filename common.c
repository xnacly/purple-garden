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
      .len = size,
      .p = s,
  };
}
