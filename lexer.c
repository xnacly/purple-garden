#include "lexer.h"
#include "common.h"
#include "mem.h"
#include "strings.h"
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#define SINGLE_TOK(t) ((Token){.type = t})

Str TOKEN_TYPE_MAP[] = {
    [T_DELIMITOR_LEFT] = STRING("T_DELIMITOR_LEFT"),
    [T_PLUS] = STRING("T_PLUS"),
    [T_MINUS] = STRING("T_MINUS"),
    [T_ASTERISKS] = STRING("T_ASTERISKS"),
    [T_SLASH] = STRING("T_SLASH"),
    [T_EQUAL] = STRING("T_EQUAL"),
    [T_LESS_THAN] = STRING("T_LESS_THAN"),
    [T_GREATER_THAN] = STRING("T_GREATER_THAN"),
    [T_EXCLAIM] = STRING("T_EXCLAIM"),
    [T_DOUBLEDOUBLEDOT] = STRING("T_DOUBLEDOUBLEDOT"),
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
    [T_VAR] = STRING("T_VAR"),
    [T_FN] = STRING("T_FN"),
    [T_MATCH] = STRING("T_MATCH"),
    [T_IDENT] = STRING("T_IDENT"),
    [T_STD] = STRING("T_STD"),
    [T_FOR] = STRING("T_FOR"),
    [T_EOF] = STRING("T_EOF"),
};

static Token *compiletime_hashes[MAX_BUILTIN_SIZE] = {0};
static Str compiletime_names[MAX_BUILTIN_SIZE] = {0};

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
Token *INTERN_EQUAL = &SINGLE_TOK(T_EQUAL);
Token *INTERN_LESS_THAN = &SINGLE_TOK(T_LESS_THAN);
Token *INTERN_GREATER_THAN = &SINGLE_TOK(T_GREATER_THAN);
Token *INTERN_EXCLAIM = &SINGLE_TOK(T_EXCLAIM);
Token *INTERN_DOUBLEDOUBLEDOT = &SINGLE_TOK(T_DOUBLEDOUBLEDOT);
Token *INTERN_FALSE = &SINGLE_TOK(T_FALSE);
Token *INTERN_TRUE = &SINGLE_TOK(T_TRUE);
Token *INTERN_VAR = &SINGLE_TOK(T_VAR);
Token *INTERN_FN = &SINGLE_TOK(T_FN);
Token *INTERN_MATCH = &SINGLE_TOK(T_MATCH);
Token *INTERN_STD = &SINGLE_TOK(T_STD);
Token *INTERN_FOR = &SINGLE_TOK(T_FOR);
Token *INTERN_EOF = &SINGLE_TOK(T_EOF);

#define NEW_BUILTIN(TEXT, INTERNED)                                            \
  {                                                                            \
    Str __s = STRING((TEXT));                                                  \
    uint32_t __hash = Str_hash(&__s) & MAX_BUILTIN_SIZE_MASK;                  \
    compiletime_hashes[__hash] = (INTERNED);                                   \
    compiletime_names[__hash] = __s;                                           \
  }

Lexer Lexer_new(Str input) {
  NEW_BUILTIN("var", INTERN_VAR)
  NEW_BUILTIN("match", INTERN_MATCH)
  NEW_BUILTIN("std", INTERN_STD);
  NEW_BUILTIN("for", INTERN_FOR);
  NEW_BUILTIN("fn", INTERN_FN);
  NEW_BUILTIN("true", INTERN_TRUE);
  NEW_BUILTIN("false", INTERN_FALSE);

  return (Lexer){
      .input = input,
      .pos = 0,
  };
}

#define CUR(L) (L->input.p[L->pos])
#define IS_ALPHANUM(CC) (alphanum_table[(uint8_t)(CC)])

// this whole block makes is_alphanum zero branch and as fast as possible
static const bool alphanum_table[256] = {['0' ... '9'] = true,
                                         ['A' ... 'Z'] = true,
                                         ['a' ... 'z'] = true,
                                         ['_'] = true};
static const bool space_table[256] = {
    [' '] = true,
    ['\t'] = true,
    ['\n'] = true,
    ['\r'] = true,
};

#define JUMP_TO_CASE goto *jump_table[(int32_t)l->input.p[l->pos]]
#define CASE(label, INTERN)                                                    \
  label : {                                                                    \
    l->pos++;                                                                  \
    return (INTERN);                                                           \
  }

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
      ['#'] = &&comment,
      ['('] = &&delimitor_left,
      [')'] = &&delimitor_right,
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
      ['<'] = &&less_than,
      ['>'] = &&greater_than,
      ['!'] = &&exclaim,
      [':'] = &&doubledoubledot,
      ['['] = &&braket_left,
      [']'] = &&braket_right,
      ['{'] = &&curly_left,
      ['}'] = &&curly_right,
      [0] = &&end,
  };
#pragma GCC diagnostic pop

  JUMP_TO_CASE;

  CASE(delimitor_left, INTERN_DELIMITOR_LEFT);
  CASE(delimitor_right, INTERN_DELIMITOR_RIGHT);
  CASE(braket_left, INTERN_BRAKET_LEFT);
  CASE(braket_right, INTERN_BRAKET_RIGHT);
  CASE(curly_left, INTERN_CURLY_LEFT);
  CASE(curly_right, INTERN_CURLY_RIGHT);
  CASE(plus, INTERN_PLUS);
  CASE(minus, INTERN_MINUS);
  CASE(slash, INTERN_SLASH);
  CASE(equal, INTERN_EQUAL);
  CASE(less_than, INTERN_LESS_THAN);
  CASE(greater_than, INTERN_GREATER_THAN);
  CASE(exclaim, INTERN_EXCLAIM);
  CASE(asterisks, INTERN_ASTERISKS);

doubledoubledot:
  l->pos++;
  if (UNLIKELY(CUR(l) != ':')) {
    fprintf(stderr, "lex: Only double double dots are allowed, not singular");
    return INTERN_EOF;
  }
  l->pos++;
  return INTERN_DOUBLEDOUBLEDOT;

number: {
  size_t start = l->pos;
  size_t i = start;
  bool is_double = false;
  uint64_t hash = FNV_OFFSET_BASIS;

#pragma GCC ivdep
  for (; i < l->input.len; i++) {
    char cc = l->input.p[i];
    hash ^= cc;
    hash *= FNV_PRIME;
    if (cc >= '0' && cc <= '9')
      continue;
    if (cc == '.') {
      if (UNLIKELY(is_double)) {
        fprintf(stderr, "lex: Invalid floating point format");
        return INTERN_EOF;
      }
      is_double = true;
      continue;
    }
    break;
  }

  l->pos = i;
  Token *n = CALL(a, request, sizeof(Token));
  *n = (Token){0};
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
  uint64_t hash = FNV_OFFSET_BASIS;

#pragma GCC ivdep
  for (char cc = CUR(l); cc > 0 && IS_ALPHANUM(cc); l->pos++, cc = CUR(l)) {
    hash ^= cc;
    hash *= FNV_PRIME;
  }

  uint64_t normalized_hash = hash & MAX_BUILTIN_SIZE_MASK;
  Token *tt = compiletime_hashes[normalized_hash];
  if (tt) {
    Str *s = &compiletime_names[normalized_hash];
    if (memcmp(s->p, l->input.p + start, s->len) == 0) {
      return tt;
    }
  }

  tt = CALL(a, request, sizeof(Token));
  tt->type = T_IDENT;
  tt->string = (Str){
      .p = l->input.p + start,
      .len = l->pos - start,
      .hash = hash,
  };

  return tt;
}

// same as string but only with leading ' and allowing everything except spaces
quoted: {
  // skip '
  l->pos++;
  size_t start = l->pos;
  uint64_t hash = FNV_OFFSET_BASIS;

#pragma GCC ivdep
  for (uint8_t cc = CUR(l); cc > 0 && !space_table[cc]; l->pos++, cc = CUR(l)) {
    hash ^= cc;
    hash *= FNV_PRIME;
  }

  size_t len = l->pos - start;
  Token *t = CALL(a, request, sizeof(Token));
  *t = (Token){0};
  t->type = T_STRING;
  t->string = (Str){
      .p = l->input.p + start,
      .len = len,
      .hash = hash,
  };
  return t;
}

string: { // TODO: handle escaped characters like \t\n\"

  // skip "
  l->pos++;
  size_t start = l->pos;
  uint64_t hash = FNV_OFFSET_BASIS;

#pragma GCC ivdep
  for (char cc = CUR(l); cc > 0 && cc != '"'; l->pos++, cc = CUR(l)) {
    hash ^= cc;
    hash *= FNV_PRIME;
  }

  if (UNLIKELY(CUR(l) != '"')) {
    Str slice = Str_slice(&l->input, l->pos, l->input.len);
    fprintf(stderr, "lex: Unterminated string near: '%.*s'", (int)slice.len,
            slice.p);
    return INTERN_EOF;
  } else {
    Token *t = CALL(a, request, sizeof(Token));
    *t = (Token){0};
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
#pragma GCC ivdep
  for (char cc = CUR(l); cc > 0 && cc != '\n'; l->pos++, cc = CUR(l)) {
  }
  JUMP_TO_CASE;

whitespace:
  l->pos++;
  JUMP_TO_CASE;

unknown: {
  uint8_t c = CUR(l);
  fprintf(stderr, "lex: Unexpected byte '%c' (0x%X) in input", c, c);
  return INTERN_EOF;
}

end:
  return INTERN_EOF;
}

#undef SINGLE_TOK

void Token_debug(Token *token) {
  if (token == NULL) {
    printf("<UNKOWN_TOKEN>");
    return;
  }
  printf("[");
  Str_debug(&TOKEN_TYPE_MAP[token->type]);
  putc(']', stdout);
  switch (token->type) {
  case T_DOUBLE:
  case T_INTEGER:
  case T_STRING:
  case T_IDENT:
    putc('[', stdout);
    Str_debug(&token->string);
    printf("]{.hash=%lu}", token->string.hash);
    break;
  case T_TRUE:
  case T_FALSE:
  default:
    break;
  }
}
