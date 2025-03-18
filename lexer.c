#include "lexer.h"
#include "common.h"

#define SINGLE_TOK(t) ((Token){.type = t})

Lexer Lexer_new(String input) {
  return (Lexer){
      .input = input,
      .pos = 0,
  };
}

static boolean at_end(Lexer *l) { return l->pos >= l->input.len; }
static char cur(Lexer *l) { return String_get(&l->input, l->pos); }
static void advance(Lexer *l) {
  char cc = cur(l);
  do {
    if (l->pos < l->input.len)
      l->pos++;
  } while (cc = cur(l), cc == ' ' || cc == '\n' || cc == '\t');
}

Token Lexer_next(Lexer *l) {
  if (at_end(l)) {
    return SINGLE_TOK(T_EOF);
  }
  char cc = cur(l);
  switch (cc) {
  case '(':
    advance(l);
    return SINGLE_TOK(T_DELIMITOR_LEFT);
  case ')':
    advance(l);
    return SINGLE_TOK(T_DELIMITOR_RIGHT);
  default:
    advance(l);
    return SINGLE_TOK(T_EOF);
  }
}

#undef SINGLE_TOK
