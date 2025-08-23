#include "lexer.h"
#include "common.h"
#include "mem.h"
#include "strings.h"
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#define SINGLE_TOK(t) ((Token){.type = t})

Str TOKEN_TYPE_MAP[] = {[T_DELIMITOR_LEFT] = STRING("T_DELIMITOR_LEFT"),
                        [T_DELIMITOR_RIGHT] = STRING("T_DELIMITOR_RIGHT"),
                        [T_BRAKET_LEFT] = STRING("T_BRAKET_LEFT"),
                        [T_BRAKET_RIGHT] = STRING("T_BRAKET_RIGHT"),
                        [T_CURLY_LEFT] = STRING("T_CURLY_LEFT"),
                        [T_CURLY_RIGHT] = STRING("T_CURLY_RIGHT"),
                        [T_STRING] = STRING("T_STRING"),
                        [T_TRUE] = STRING("T_TRUE"),
                        [T_FALSE] = STRING("T_FALSE"),
                        [T_DOUBLE] = STRING("T_DOUBLE"),
                        [T_INTEGER] = STRING("T_INTEGER"),
                        [T_BUILTIN] = STRING("T_BUILTIN"),
                        [T_IDENT] = STRING("T_IDENT"),
                        [T_PLUS] = STRING("T_PLUS"),
                        [T_MINUS] = STRING("T_MINUS"),
                        [T_ASTERISKS] = STRING("T_ASTERISKS"),
                        [T_SLASH] = STRING("T_SLASH"),
                        [T_EQUAL] = STRING("T_EQUAL"),
                        [T_EOF] = STRING("T_EOF")};

#if DEBUG
void Token_debug(Token *token) {
  ASSERT(token->type <= T_EOF, "Out of bounds type, SHOULD NEVER HAPPEN");
  putc('[', stdout);
  Str_debug(&TOKEN_TYPE_MAP[token->type]);
  putc(']', stdout);
  switch (token->type) {
  case T_DOUBLE:
  case T_INTEGER:
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
      .true_hash = Str_hash(&STRING("true")),
      .false_hash = Str_hash(&STRING("false")),
  };
}

#define cur(L) (L->input.p[L->pos])

__attribute__((always_inline)) inline static bool is_alphanum(uint8_t cc) {
  uint8_t lower = cc | 0x20;
  bool is_alpha = (lower >= 'a' && lower <= 'z');
  bool is_digit = (cc >= '0' && cc <= '9');
  return is_alpha || is_digit || cc == '_' || cc == '-';
}

// we can "intern" these, since all of them are the same, regardless of position
Token *INTERN_DELIMITOR_LEFT = &SINGLE_TOK(T_DELIMITOR_LEFT);
Token *INTERN_DELIMITOR_RIGHT = &SINGLE_TOK(T_DELIMITOR_RIGHT);
Token *INTERN_BRAKET_LEFT = &SINGLE_TOK(T_BRAKET_LEFT);
Token *INTERN_BRAKET_RIGHT = &SINGLE_TOK(T_BRAKET_RIGHT);
Token *INTERN_CURLY_LEFT = &SINGLE_TOK(T_CURLY_LEFT);
Token *INTERN_CURLY_RIGHT = &SINGLE_TOK(T_CURLY_RIGHT);
Token *INTERN_MINUS = &SINGLE_TOK(T_MINUS);
Token *INTERN_PLUS = &SINGLE_TOK(T_PLUS);
Token *INTERN_ASTERISKS = &SINGLE_TOK(T_ASTERISKS);
Token *INTERN_SLASH = &SINGLE_TOK(T_SLASH);
Token *INTERN_FALSE = &SINGLE_TOK(T_FALSE);
Token *INTERN_TRUE = &SINGLE_TOK(T_TRUE);
Token *INTERN_EQUAL = &SINGLE_TOK(T_EQUAL);
Token *INTERN_EOF = &SINGLE_TOK(T_EOF);

// TODO: lexer needs a hash based string and ident interning model, the current
// falls apart after around 1 mio identifiers
Token *Lexer_next(Lexer *l, Allocator *a) {
  // empty input
  if (l->input.len == 0) {
    return INTERN_EOF;
  }

#pragma GCC diagnostic push
  // We know what we're doing, so this is fine:
  //
  // we assign unknown to all and overwrite these to make sure an invalid
  // index is not a unassigned memory access.
#pragma GCC diagnostic ignored "-Woverride-init"
  static void *jump_table[256] = {
      [0 ... 255] = &&unknown,
      [' '] = &&whitespace,
      ['\t'] = &&whitespace,
      ['\n'] = &&whitespace,
      [';'] = &&comment,
      ['('] = &&delimitor_left,
      [')'] = &&delimitor_right,
      ['@'] = &&builtin,
      ['.'] = &&number,
      ['0' ... '9'] = &&number,
      ['a' ... 'z'] = &&ident,
      ['A' ... 'Z'] = &&ident,
      ['_'] = &&ident,
      ['\''] = &&quoted,
      ['"'] = &&string,
      ['+'] = &&plus,
      ['-'] = &&minus,
      ['/'] = &&slash,
      ['*'] = &&asterisks,
      ['='] = &&equal,
      ['['] = &&braket_left,
      [']'] = &&braket_right,
      ['{'] = &&curly_left,
      ['}'] = &&curly_right,
      [0] = &&end,
  };
#pragma GCC diagnostic pop

#define JUMP_TARGET goto *jump_table[(int32_t)l->input.p[l->pos]]

  JUMP_TARGET;

delimitor_left:
  l->pos++;
  return INTERN_DELIMITOR_LEFT;

delimitor_right:
  l->pos++;
  return INTERN_DELIMITOR_RIGHT;

braket_left:
  l->pos++;
  return INTERN_BRAKET_LEFT;

braket_right:
  l->pos++;
  return INTERN_BRAKET_RIGHT;

curly_left:
  l->pos++;
  return INTERN_CURLY_LEFT;

curly_right:
  l->pos++;
  return INTERN_CURLY_RIGHT;

builtin: {
  l->pos++;
  // not an ident after @, this is shit
  if (!is_alphanum(cur(l))) {
    return INTERN_EOF;
  }
  size_t start = l->pos;
  size_t hash = FNV_OFFSET_BASIS;
  for (char cc = cur(l); cc > 0 && is_alphanum(cc); l->pos++, cc = cur(l)) {
    hash ^= cc;
    hash *= FNV_PRIME;
  }

  size_t len = l->pos - start;
  Str s = (Str){
      .p = l->input.p + start,
      .len = len,
      .hash = hash,
  };
  Token *b = CALL(a, request, sizeof(Token));
  b->string = s;
  b->type = T_BUILTIN;
  return b;
}

plus:
  l->pos++;
  return INTERN_PLUS;

minus:
  l->pos++;
  return INTERN_MINUS;

slash:
  l->pos++;
  return INTERN_SLASH;

equal:
  l->pos++;
  return INTERN_EQUAL;

asterisks:
  l->pos++;
  return INTERN_ASTERISKS;

number: {
  size_t start = l->pos;
  size_t i = start;
  bool is_double = false;
  size_t hash = FNV_OFFSET_BASIS;
  for (; i < l->input.len; i++) {
    char cc = l->input.p[i];
    hash ^= cc;
    hash *= FNV_PRIME;
    if (cc >= '0' && cc <= '9')
      continue;
    if (cc == '.') {
      ASSERT(!is_double, "Two dots in double");
      is_double = true;
      continue;
    }
    break;
  }

  l->pos = i;
  Token *n = CALL(a, request, sizeof(Token));
  n->string = (Str){
      .p = l->input.p + start,
      .len = i - start,
      .hash = hash,
  };
  if (is_double) {
    n->type = T_DOUBLE;
  } else {
    n->type = T_INTEGER;
  }

  return n;
}

ident: {
  size_t start = l->pos;
  size_t hash = FNV_OFFSET_BASIS;
  for (char cc = cur(l); cc > 0 && is_alphanum(cc); l->pos++, cc = cur(l)) {
    hash ^= cc;
    hash *= FNV_PRIME;
  }

  size_t len = l->pos - start;
  Token *t;
  if (hash == l->true_hash) {
    t = INTERN_TRUE;
  } else if (hash == l->false_hash) {
    t = INTERN_FALSE;
  } else {
    t = CALL(a, request, sizeof(Token));
    t->type = T_IDENT;
    t->string = (Str){
        .p = l->input.p + start,
        .len = len,
        .hash = hash,
    };
  }
  return t;
}

// same as string but only with leading '
quoted: {
  // skip '
  l->pos++;
  size_t start = l->pos;
  size_t hash = FNV_OFFSET_BASIS;
  for (char cc = cur(l); cc > 0 && is_alphanum(cc); l->pos++, cc = cur(l)) {
    hash ^= cc;
    hash *= FNV_PRIME;
  }

  size_t len = l->pos - start;
  Token *t;
  t = CALL(a, request, sizeof(Token));
  t->type = T_STRING;
  t->string = (Str){
      .p = l->input.p + start,
      .len = len,
      .hash = hash,
  };
  return t;
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

  if (UNLIKELY(cur(l) != '"')) {
    Str slice = Str_slice(&l->input, l->pos, l->input.len);
    fprintf(stderr, "lex: Unterminated string near: '%.*s'", (int)slice.len,
            slice.p);
    return INTERN_EOF;
  } else {
    Token *t = CALL(a, request, sizeof(Token));
    t->type = T_STRING;
    t->string = (Str){
        .p = l->input.p + start,
        .len = l->pos - start,
        .hash = hash,
    };
    // skip "
    l->pos++;
    return t;
  }
}

comment:
  for (char cc = cur(l); cc > 0 && cc != '\n'; l->pos++, cc = cur(l)) {
  }
  JUMP_TARGET;

whitespace:
  l->pos++;
  JUMP_TARGET;

unknown: {
  uint8_t c = cur(l);
  ASSERT(0, "Unexpected byte '%c' (0x%X) in input", c, c)
}

end:
  return INTERN_EOF;
}

#undef SINGLE_TOK
