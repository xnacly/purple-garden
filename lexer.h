#pragma once

#include "common.h"
#include "mem.h"
#include "string.h"

typedef enum {
  // end marker, specifically 0 to enable if(Token::type)
  T_EOF,
  // (
  T_DELIMITOR_LEFT = 1,
  // assigned OP numbers because we directly map these in the compiler, see
  // vm.h#VM_OP
  T_PLUS = 2,
  T_MINUS = 3,
  T_ASTERISKS = 4,
  T_SLASH = 5,
  // =
  T_EQUAL = 6,
  // <
  T_LESS_THAN = 7,
  // >
  T_GREATER_THAN = 8,
  // !
  T_EXCLAIM,
  // ::
  T_DOUBLEDOUBLEDOT,
  // )
  T_DELIMITOR_RIGHT,
  // [
  T_BRAKET_LEFT,
  // ]
  T_BRAKET_RIGHT,
  // {
  T_CURLY_LEFT,
  // }
  T_CURLY_RIGHT,
  // anything between ""
  T_STRING,
  T_TRUE,
  T_FALSE,
  // floating point numbers
  T_DOUBLE,
  // whole numbers
  T_INTEGER,
  // builtins in the format @<builtin>, but only
  T_BUILTIN,
  // compile time builtins
  T_VAR,
  T_FN,
  T_MATCH,
  T_STD,
  // any identifier
  T_IDENT,
} TokenType;

// TOKEN_TYPE_MAP allows for mapping TokenType to its string representation
extern Str TOKEN_TYPE_MAP[];

typedef struct __Token {
  TokenType type;
  // stores all values for T_STRING,T_IDENT,T_INTEGER and T_DOUBLE
  Str string;
} Token;

typedef struct {
  Str input;
  size_t pos;
} Lexer;

Lexer Lexer_new(Str input);
Token *Lexer_next(Lexer *l, Allocator *a);
// Token_debug will print a debug representation of token to stdout
void Token_debug(Token *token);
