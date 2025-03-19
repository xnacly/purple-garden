#include "common.h"
#include <string.h>

String TOKEN_TYPE_MAP[] = {[T_DELIMITOR_LEFT] = STRING("T_DELIMITOR_LEFT"),
                           [T_DELIMITOR_RIGHT] = STRING("T_DELIMITOR_RIGHT"),
                           [T_STRING] = STRING("T_STRING"),
                           [T_NUMBER] = STRING("T_NUMBER"),
                           [T_IDENT] = STRING("T_IDENT"),
                           [T_EOF] = STRING("T_EOF")};

char String_get(String *str, size_t index) {
  if (index >= str->len) {
    return -1;
  }
  return str->p[index];
}

char *String_to(String *str) { return str->p; }

String String_from(char *s) {
  return (String){
      .len = strlen(s),
      .p = s,
  };
}
