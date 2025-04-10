#include "lexer.h"
#include "common.h"
#include <stdio.h>
#include <stdlib.h>

#define SINGLE_TOK(t) ((Token){.type = t})

Str TOKEN_TYPE_MAP[] = {[T_DELIMITOR_LEFT] = STRING("T_DELIMITOR_LEFT"),
                        [T_DELIMITOR_RIGHT] = STRING("T_DELIMITOR_RIGHT"),
                        [T_STRING] = STRING("T_STRING"),
                        [T_TRUE] = STRING("T_TRUE"),
                        [T_FALSE] = STRING("T_FALSE"),
                        [T_NUMBER] = STRING("T_NUMBER"),
                        [T_BUILTIN] = STRING("T_BUILTIN"),
                        [T_IDENT] = STRING("T_IDENT"),
                        [T_PLUS] = STRING("T_PLUS"),
                        [T_MINUS] = STRING("T_MINUS"),
                        [T_ASTERISKS] = STRING("T_ASTERISKS"),
                        [T_SLASH] = STRING("T_SLASH"),
                        [T_EOF] = STRING("T_EOF")};

#if DEBUG
void Token_debug(Token *token) {
  putc('[', stdout);
  Str_debug(&TOKEN_TYPE_MAP[token->type]);
  putc(']', stdout);
  switch (token->type) {
  case T_NUMBER:
    printf("(%f)", token->number);
    break;
  case T_STRING:
  case T_BUILTIN:
  case T_IDENT:
    putc('[', stdout);
    Str_debug(&token->string);
    putc(']', stdout);
    break;
  case T_TRUE:
  case T_FALSE:
  default:
    break;
  }
}
#endif

Lexer Lexer_new(Str input) {
  return (Lexer){
      .input = input,
      .pos = 0,
  };
}

static bool at_end(Lexer *l) { return l->pos >= l->input.len; }
static char cur(Lexer *l) { return Str_get(&l->input, l->pos); }
static bool is_whitespace(char cc) {
  return cc == ' ' || cc == '\n' || cc == '\t';
}
static bool is_ident(char cc) {
  return (cc >= 'a' && cc <= 'z') || (cc >= 'A' && cc <= 'Z') || cc == '_' ||
         cc == '-';
}

static void advance(Lexer *l) {
  do {
    if (l->pos < l->input.len)
      l->pos++;
  } while (is_whitespace(cur(l)));
}

static void skip_whitespace(Lexer *l) {
  while (is_whitespace(cur(l))) {
    l->pos++;
  }
}

static Token num(Lexer *l) {
  size_t start = l->pos;
  for (char cc = cur(l); cc > 0 && ((cc >= '0' && cc <= '9') || cc == '.' ||
                                    cc == 'e' || cc == '+' || cc == '-');
       l->pos++, cc = cur(l))
    ;
  char *endptr;
  double d = strtod(l->input.p + start, &endptr);
  ASSERT(endptr != (l->input.p + start), "lex: Failed to parse number")

  skip_whitespace(l);
  return (Token){
      .type = T_NUMBER,
      .number = d,
  };
}

static Token string(Lexer *l) {
  // skip "
  l->pos++;
  size_t start = l->pos;
  for (char cc = cur(l); cc > 0 && cc != '"'; l->pos++, cc = cur(l)) {
    // escape handling
    // if (cc == '\\') {
    //   // manual advance to skip \ and next char, only one because the post
    //   // section of the for loop already skips cc
    //   l->pos += 1;
    // }
  }

  if (cur(l) != '"') {
    Str slice = Str_slice(&l->input, l->pos, l->input.len);
    fprintf(stderr, "lex: Unterminated string near: '%.*s'", (int)slice.len,
            slice.p);
    return SINGLE_TOK(T_EOF);
  }

  Str s = Str_slice(&l->input, start, l->pos);
  // skip "
  l->pos++;
  return (Token){
      .type = T_STRING,
      .string = s,
  };
}

static Token ident(Lexer *l) {
  size_t start = l->pos;
  for (char cc = cur(l); cc > 0 && is_ident(cc); l->pos++, cc = cur(l))
    ;
  Str s = Str_slice(&l->input, start, l->pos);
  skip_whitespace(l);
  if (s.len == 4 &&
      (s.p[0] == 't' && s.p[1] == 'r' && s.p[2] == 'u' && s.p[3] == 'e')) {
    return (Token){
        .type = T_TRUE,
    };
  } else if (s.len == 5 && (s.p[0] == 'f' && s.p[1] == 'a' && s.p[2] == 'l' &&
                            s.p[3] == 's' && s.p[4] == 'e')) {
    return (Token){
        .type = T_FALSE,
    };
  } else {
    return (Token){
        .type = T_IDENT,
        .string = s,
    };
  }
}

Token Lexer_next(Lexer *l) {
  skip_whitespace(l);
  if (at_end(l)) {
    return SINGLE_TOK(T_EOF);
  }
  char cc = cur(l);
  switch (cc) {
  case ';':
    for (cc = cur(l); cc > 0 && cc != '\n'; l->pos++, cc = cur(l)) {
    }
    return Lexer_next(l);
  case '@': {
    l->pos++;
    // not an ident after @, this is shit
    if (!is_ident(cur(l))) {
      return SINGLE_TOK(T_EOF);
    }
    Token a = ident(l);
    a.type = T_BUILTIN;
    return a;
  }
  case '+':
    l->pos++;
    return SINGLE_TOK(T_PLUS);
  case '-':
    l->pos++;
    return SINGLE_TOK(T_MINUS);
  case '*':
    l->pos++;
    return SINGLE_TOK(T_ASTERISKS);
  case '/':
    l->pos++;
    return SINGLE_TOK(T_SLASH);
  case '"':
    return string(l);
  case '(':
    l->pos++;
    return SINGLE_TOK(T_DELIMITOR_LEFT);
  case ')':
    l->pos++;
    return SINGLE_TOK(T_DELIMITOR_RIGHT);
    // EOF case
  case -1:
    l->pos++;
    return SINGLE_TOK(T_EOF);
  default:
    if ((cc >= '0' && cc <= '9') || cc == '.') {
      return num(l);
    } else if (is_ident(cc)) {
      return ident(l);
    }
    printf("lex: Unknown token '%c' at ", cur(l));
    Str rest = Str_slice(&l->input, l->pos, l->input.len);
    Str_debug(&rest);
    putc('\n', stdout);
    l->pos++;
    return SINGLE_TOK(T_EOF);
  }
}

#undef SINGLE_TOK
