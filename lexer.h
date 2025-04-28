#ifndef LEXER_H
#define LEXER_H

#include "common.h"
#include "mem.h"
#include "string.h"

typedef enum {
  // (
  T_DELIMITOR_LEFT = 1,
  // assigned OP numbers because we directly map these in the compiler, see
  // vm.h#VM_OP
  T_PLUS = 2,
  T_MINUS = 3,
  T_ASTERISKS = 4,
  T_SLASH = 5,
  // )
  T_DELIMITOR_RIGHT,
  // [
  T_BRAKET_LEFT,
  // ]
  T_BRAKET_RIGHT,
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
  // end marker
  T_EOF,
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
size_t Lexer_all(Lexer *l, Allocator *a, Token **out);

#endif
