#ifndef COMMON_H
#define COMMON_H

#include <stddef.h>

typedef short boolean;
#define true ((boolean)1)
#define false ((boolean)0)

// String is a simple wrapper around C char arrays, providing constant time
// length access
typedef struct {
  char *p;
  size_t len;
} String;

#define STRING(str) ((String){.len = sizeof(str), .p = str})

// String_get enables accessing a character at a position of a string with
// bounds checking
char String_get(String *str, size_t index);

// String_to converts str to a c like string
char *String_to(String *str);

// String_from converts s to a String
String String_from(char *s);

// String_slice returns a slice of str from start to end (causes allocation)
String String_slice(String *str, size_t start, size_t end);

typedef enum {
  // (
  T_DELIMITOR_LEFT,
  // )
  T_DELIMITOR_RIGHT,
  // anything between ""
  T_STRING,
  // floating point numbers
  T_NUMBER,
  // any identifier
  T_IDENT,
  // end marker
  T_EOF
} TokenType;

// TOKEN_TYPE_MAP allows for mapping TokenType to its string representation
extern String TOKEN_TYPE_MAP[];

typedef struct {
  TokenType type;
  union {
    // filled when .type=T_STRING|T_IDENT
    String string;
    // filled when .type=T_NUMBER
    double num;
  };
} Token;

// Token_destroy deallocates a Token, if allocated
void Token_destroy(Token *token);
// Token_debug will print a debug representation of token to stdout
void Token_debug(Token *token);

#endif
