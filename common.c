#include "common.h"

char *TOKEN_TYPE_MAP[] = {[T_DELIMITOR_LEFT] = "T_DELIMITOR_LEFT",
                          [T_DELIMITOR_RIGHT] = "T_DELIMITOR_RIGHT",
                          [T_STRING] = "T_STRING",
                          [T_NUMBER] = "T_NUMBER",
                          [T_IDENT] = "T_IDENT",
                          [T_EOF] = "T_EOF"};

char String_get(String *str, size_t index) {
  if (index >= str->len) {
    return -1;
  }
  return str->p[index];
}
