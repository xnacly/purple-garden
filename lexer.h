#ifndef LEXER_H
#define LEXER_H

#include "common.h"
#include "string.h"

typedef enum {
  // (
  T_DELIMITOR_LEFT,
  // )
  T_DELIMITOR_RIGHT,
  // anything between ""
  T_STRING,
  T_TRUE,
  T_FALSE,
  // floating point numbers
  T_NUMBER,
  // builtins in the format @<builtin>
  T_BUILTIN,
  // any identifier
  T_IDENT,
  //
  T_PLUS,
  //
  T_MINUS,
  //
  T_ASTERISKS,
  //
  T_SLASH,
  // end marker
  T_EOF
} TokenType;

// TOKEN_TYPE_MAP allows for mapping TokenType to its string representation
extern Str TOKEN_TYPE_MAP[];

typedef struct {
  TokenType type;
  union {
    // filled when .type=T_STRING|T_IDENT
    Str string;
    // filled when .type=T_NUMBER
    double number;
  };
} Token;

#if DEBUG
// Token_debug will print a debug representation of token to stdout
void Token_debug(Token *token);
#endif

typedef struct {
  Str input;
  size_t pos;
} Lexer;

Lexer Lexer_new(Str input);
Token Lexer_next(Lexer *l);

#endif
