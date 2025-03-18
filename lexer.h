#ifndef LEXER_H
#define LEXER_H

#include "common.h"
#include "string.h"

typedef struct {
  String input;
  size_t pos;
} Lexer;

Lexer Lexer_new(String input);
Token Lexer_next(Lexer *l);

#endif
