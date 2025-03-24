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
  // true or false
  T_BOOLEAN,
  // floating point numbers
  T_NUMBER,
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
extern String TOKEN_TYPE_MAP[];

typedef struct {
  TokenType type;
  union {
    // filled when .type=T_STRING|T_IDENT
    String string;
    // filled when .type=T_NUMBER
    double number;
    // filled when .type=T_BOOLEAN
    bool boolean;
  };
} Token;

// Token_destroy deallocates a Token, if allocated
void Token_destroy(Token *token);
// Token_debug will print a debug representation of token to stdout
void Token_debug(Token *token);

typedef struct {
  String input;
  size_t pos;
} Lexer;

Lexer Lexer_new(String input);
Token Lexer_next(Lexer *l);

#endif
