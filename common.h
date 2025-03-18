#ifndef COMMON_H
#define COMMON_H

#include <stddef.h>

typedef short boolean;
#define true ((boolean)1)
#define false ((boolean)0)

typedef struct {
  char *p;
  size_t len;
} String;

#define STRING(str) ((String){.len = sizeof(str), .p = str})

char String_get(String *str, size_t index);

typedef enum {
  T_DELIMITOR_LEFT,
  T_DELIMITOR_RIGHT,
  T_STRING,
  T_NUMBER,
  T_IDENT,
  T_EOF
} TokenType;

extern char *TOKEN_TYPE_MAP[];

typedef struct {
  TokenType type;
  union {
    String string;
    double num;
  };
} Token;

#endif
