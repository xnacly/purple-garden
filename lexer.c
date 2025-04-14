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
  ASSERT(token->type <= T_EOF, "Out of bounds type, SHOULD NEVER HAPPEN");
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

#define cur(L) ((L->pos < L->input.len) ? L->input.p[L->pos] : 0)

inline static bool is_ident(char cc) {
  return (cc >= 'a' && cc <= 'z') || (cc >= 'A' && cc <= 'Z') || cc == '_' ||
         cc == '-';
}

size_t Lexer_all(Lexer *l, Token *out) {
  ASSERT(out != NULL, "Failed to allocate token list");
  size_t count = 0;
  static void *jump_table[256] = {
      ['('] = &&delimitor_left, [')'] = &&delimitor_right,
      ['@'] = &&builtin,        ['+'] = &&plus,
      ['-'] = &&minus,          ['/'] = &&slash,
      ['*'] = &&asterisks,      [' '] = &&whitespace,
      ['\t'] = &&whitespace,    ['\n'] = &&whitespace,
      [';'] = &&comment,        ['.'] = &&number,
      ['0' ... '9'] = &&number, ['a' ... 'z'] = &&ident,
      ['A' ... 'Z'] = &&ident,  ['_'] = &&ident,
      ['"'] = &&string,         [0] = &&end,
  };

#define JUMP_TARGET                                                            \
  do {                                                                         \
    int c = cur(l);                                                            \
    ASSERT(!(c & 0x80), "Non-ASCII input!");                                   \
    void *target = jump_table[c];                                              \
    ASSERT(target != NULL, "Unknown character in lexer: '%c'(%d)",             \
           l->input.p[l->pos], l->input.p[l->pos]);                            \
    goto *target;                                                              \
  } while (0);

  JUMP_TARGET;

delimitor_left:
  out[count++] = SINGLE_TOK(T_DELIMITOR_LEFT);
  l->pos++;
  JUMP_TARGET;

delimitor_right:
  out[count++] = SINGLE_TOK(T_DELIMITOR_RIGHT);
  l->pos++;
  JUMP_TARGET;

builtin: {
  l->pos++;
  // not an ident after @, this is shit
  if (!is_ident(cur(l))) {
    out[count++] = SINGLE_TOK(T_EOF);
  }
  size_t start = l->pos;
  size_t hash = FNV_OFFSET_BASIS;
  for (char cc = cur(l); cc > 0 && is_ident(cc); l->pos++, cc = cur(l)) {
    hash ^= cc;
    hash *= FNV_PRIME;
  }

  size_t len = l->pos - start;
  Str s = (Str){
      .p = l->input.p + start,
      .len = len,
      .hash = hash & GLOBAL_MASK,
  };
  Token a = (Token){
      .type = T_IDENT,
      .string = s,
  };
  a.type = T_BUILTIN;
  out[count++] = a;
  JUMP_TARGET;
}

plus:
  out[count++] = SINGLE_TOK(T_PLUS);
  l->pos++;
  JUMP_TARGET;

minus:
  out[count++] = SINGLE_TOK(T_MINUS);
  l->pos++;
  JUMP_TARGET;

slash:
  out[count++] = SINGLE_TOK(T_SLASH);
  l->pos++;
  JUMP_TARGET;

asterisks:
  out[count++] = SINGLE_TOK(T_ASTERISKS);
  l->pos++;
  JUMP_TARGET;

number: {
  size_t start = l->pos;
  char *input_start = l->input.p + start;
  // PERF: using strtol if number is not a floating point number ~1.405243x
  // faster (-28.84%)
  bool is_double = false;
  for (char cc = cur(l); cc > 0; l->pos++, cc = cur(l)) {
    if (cc == '.' || cc == 'e') {
      is_double = true;
    } else if (!((cc >= '0' && cc <= '9') || cc == '+' || cc == '-')) {
      break;
    }
  }

  // limit how far strto* can go in the input, because no strings we use are 0
  // terminated (strings::Str) and we know the length of the number
  char *endptr = input_start + l->pos;

  double d;
  if (is_double) {
    d = strtod(input_start, &endptr);
  } else {
    d = (double)strtol(input_start, &endptr, 10);
  }
  ASSERT(endptr != input_start, "lex: Failed to parse number")
  out[count++] = (Token){
      .type = T_NUMBER,
      .number = d,
  };
  JUMP_TARGET;
}

ident: {
  size_t start = l->pos;
  size_t hash = FNV_OFFSET_BASIS;
  for (char cc = cur(l); cc > 0 && is_ident(cc); l->pos++, cc = cur(l)) {
    hash ^= cc;
    hash *= FNV_PRIME;
  }

  size_t len = l->pos - start;
  Token t;
  if (len == 4 &&
      (l->input.p[start + 0] == 't' && l->input.p[start + 1] == 'r' &&
       l->input.p[start + 2] == 'u' && l->input.p[start + 3] == 'e')) {
    t = (Token){
        .type = T_TRUE,
    };
  } else if (len == 5 &&
             (l->input.p[start + 0] == 'f' && l->input.p[start + 1] == 'a' &&
              l->input.p[start + 2] == 'l' && l->input.p[start + 3] == 's' &&
              l->input.p[start + 4] == 'e')) {
    t = (Token){
        .type = T_FALSE,
    };
  } else {
    Str s = (Str){
        .p = l->input.p + start,
        .len = len,
        .hash = hash & GLOBAL_MASK,
    };
    t = (Token){
        .type = T_IDENT,
        .string = s,
    };
  }
  out[count++] = t;
  JUMP_TARGET;
}

string: {
  // skip "
  l->pos++;
  size_t start = l->pos;
  size_t hash = FNV_OFFSET_BASIS;
  for (char cc = cur(l); cc > 0 && cc != '"'; l->pos++, cc = cur(l)) {
    hash ^= cc;
    hash *= FNV_PRIME;
  }

  if (cur(l) != '"') {
    Str slice = Str_slice(&l->input, l->pos, l->input.len);
    fprintf(stderr, "lex: Unterminated string near: '%.*s'", (int)slice.len,
            slice.p);
    out[count++] = SINGLE_TOK(T_EOF);
  } else {
    Str s = (Str){
        .p = l->input.p + start,
        .len = l->pos - start,
        .hash = hash & GLOBAL_MASK,
    };
    // skip "
    l->pos++;
    out[count++] = (Token){
        .type = T_STRING,
        .string = s,
    };
  }
  JUMP_TARGET;
}

comment:
  for (char cc = cur(l); cc > 0 && cc != '\n'; l->pos++, cc = cur(l)) {
  }
  JUMP_TARGET;

whitespace:
  l->pos++;
  JUMP_TARGET;

end:
  out[count++] = SINGLE_TOK(T_EOF);
  return count;
}

#undef SINGLE_TOK
