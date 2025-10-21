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
    [T_EOF] = STRING("T_EOF"),
};

static Token *compiletime_hashes[MAX_BUILTIN_SIZE] = {0};

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
Token *INTERN_EOF = &SINGLE_TOK(T_EOF);

Lexer Lexer_new(Str input) {
  compiletime_hashes[Str_hash(&STRING("var")) & MAX_BUILTIN_SIZE_MASK] =
      INTERN_VAR;
  compiletime_hashes[Str_hash(&STRING("match")) & MAX_BUILTIN_SIZE_MASK] =
      INTERN_MATCH;
  compiletime_hashes[Str_hash(&STRING("std")) & MAX_BUILTIN_SIZE_MASK] =
      INTERN_STD;
  compiletime_hashes[Str_hash(&STRING("fn")) & MAX_BUILTIN_SIZE_MASK] =
      INTERN_FN;
  compiletime_hashes[Str_hash(&STRING("true")) & MAX_BUILTIN_SIZE_MASK] =
      INTERN_TRUE;
  compiletime_hashes[Str_hash(&STRING("false")) & MAX_BUILTIN_SIZE_MASK] =
      INTERN_FALSE;

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

#define JUMP_TARGET goto *jump_table[(int32_t)l->input.p[l->pos]]
#define SYMBOL(label, INTERN)                                                  \
  label : {                                                                    \
    l->pos++;                                                                  \
    return (INTERN);                                                           \
  }

  JUMP_TARGET;

  SYMBOL(delimitor_left, INTERN_DELIMITOR_LEFT);
  SYMBOL(delimitor_right, INTERN_DELIMITOR_RIGHT);
  SYMBOL(braket_left, INTERN_BRAKET_LEFT);
  SYMBOL(braket_right, INTERN_BRAKET_RIGHT);
  SYMBOL(curly_left, INTERN_CURLY_LEFT);
  SYMBOL(curly_right, INTERN_CURLY_RIGHT);
  SYMBOL(plus, INTERN_PLUS);
  SYMBOL(minus, INTERN_MINUS);
  SYMBOL(slash, INTERN_SLASH);
  SYMBOL(equal, INTERN_EQUAL);
  SYMBOL(less_than, INTERN_LESS_THAN);
  SYMBOL(greater_than, INTERN_GREATER_THAN);
  SYMBOL(exclaim, INTERN_EQUAL);
  SYMBOL(asterisks, INTERN_ASTERISKS);

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

#pragma GCC unroll 32
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

#pragma GCC unroll 32
  for (char cc = CUR(l); cc > 0 && IS_ALPHANUM(cc); l->pos++, cc = CUR(l)) {
    hash ^= cc;
    hash *= FNV_PRIME;
  }

  Token *tt = compiletime_hashes[hash & MAX_BUILTIN_SIZE_MASK];

  if (tt == NULL) {
    tt = CALL(a, request, sizeof(Token));
    tt->type = T_IDENT;
    tt->string = (Str){
        .p = l->input.p + start,
        .len = l->pos - start,
        .hash = hash,
    };
  }

  return tt;
}

// same as string but only with leading ' and allowing everything except spaces
quoted: {
  // skip '
  l->pos++;
  size_t start = l->pos;
  uint64_t hash = FNV_OFFSET_BASIS;

#pragma GCC unroll 32
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

string: {
  // skip "
  l->pos++;
  size_t start = l->pos;
  uint64_t hash = FNV_OFFSET_BASIS;

#pragma GCC unroll 32
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
  for (char cc = CUR(l); cc > 0 && cc != '\n'; l->pos++, cc = CUR(l)) {
  }
  JUMP_TARGET;

whitespace:
  l->pos++;
  JUMP_TARGET;

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
  case T_BUILTIN:
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
