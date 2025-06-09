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
    printf("(%g)", token->floating);
    break;
  case T_INTEGER:
    printf("(%ld)", token->integer);
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

inline static bool is_alphanum(uint8_t cc) {
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
Token *INTERN_MINUS = &SINGLE_TOK(T_MINUS);
Token *INTERN_PLUS = &SINGLE_TOK(T_PLUS);
Token *INTERN_ASTERISKS = &SINGLE_TOK(T_ASTERISKS);
Token *INTERN_SLASH = &SINGLE_TOK(T_SLASH);
Token *INTERN_FALSE = &SINGLE_TOK(T_FALSE);
Token *INTERN_TRUE = &SINGLE_TOK(T_TRUE);
Token *INTERN_EQUAL = &SINGLE_TOK(T_EQUAL);
Token *INTERN_EOF = &SINGLE_TOK(T_EOF);

size_t Lexer_all(Lexer *l, Allocator *a, Token **out) {
  ASSERT(out != NULL, "Failed to allocate token list");

  size_t true_hash = Str_hash(&STRING("true"));
  size_t false_hash = Str_hash(&STRING("false"));

  size_t count = 0;
  static void *jump_table[256] = {
      [0 ... 255] = &&unknown,
      ['('] = &&delimitor_left,
      [')'] = &&delimitor_right,
      ['['] = &&braket_left,
      [']'] = &&braket_right,
      ['@'] = &&builtin,
      ['+'] = &&plus,
      ['-'] = &&minus,
      ['/'] = &&slash,
      ['*'] = &&asterisks,
      ['='] = &&equal,
      [' '] = &&whitespace,
      ['\t'] = &&whitespace,
      ['\n'] = &&whitespace,
      [';'] = &&comment,
      ['.'] = &&number,
      ['0' ... '9'] = &&number,
      ['a' ... 'z'] = &&ident,
      ['A' ... 'Z'] = &&ident,
      ['_'] = &&ident,
      ['"'] = &&string,
      [0] = &&end,
  };

#define JUMP_TARGET goto *jump_table[(int8_t)l->input.p[l->pos]]

  JUMP_TARGET;

delimitor_left:
  out[count++] = INTERN_DELIMITOR_LEFT;
  l->pos++;
  JUMP_TARGET;

delimitor_right:
  out[count++] = INTERN_DELIMITOR_RIGHT;
  l->pos++;
  JUMP_TARGET;

braket_left:
  out[count++] = INTERN_BRAKET_LEFT;
  l->pos++;
  JUMP_TARGET;

braket_right:
  out[count++] = INTERN_BRAKET_RIGHT;
  l->pos++;
  JUMP_TARGET;

builtin: {
  l->pos++;
  // not an ident after @, this is shit
  if (!is_alphanum(cur(l))) {
    out[count++] = INTERN_EOF;
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
  Token *b = a->request(a->ctx, sizeof(Token));
  b->string = s;
  b->type = T_BUILTIN;
  out[count++] = b;
  JUMP_TARGET;
}

plus:
  out[count++] = INTERN_PLUS;
  l->pos++;
  JUMP_TARGET;

minus:
  out[count++] = INTERN_MINUS;
  l->pos++;
  JUMP_TARGET;

slash:
  out[count++] = INTERN_SLASH;
  l->pos++;
  JUMP_TARGET;

equal:
  out[count++] = INTERN_EQUAL;
  l->pos++;
  JUMP_TARGET;

asterisks:
  out[count++] = INTERN_ASTERISKS;
  l->pos++;
  JUMP_TARGET;

number: {
  size_t start = l->pos;
  size_t i = start;
  bool is_double = false;
  for (; i < l->input.len; i++) {
    char cc = l->input.p[i];
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
  Str s = {
      .p = l->input.p + start,
      .len = i - start,
  };
  Token *n = a->request(a->ctx, sizeof(Token));
  if (is_double) {
    n->type = T_DOUBLE;
    n->floating = Str_to_double(&s);
  } else {
    n->type = T_INTEGER;
    n->integer = Str_to_int64_t(&s);
  }

  out[count++] = n;
  JUMP_TARGET;
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
  if (hash == true_hash) {
    t = INTERN_TRUE;
  } else if (hash == false_hash) {
    t = INTERN_FALSE;
  } else {
    t = a->request(a->ctx, sizeof(Token));
    t->type = T_IDENT;
    t->string = (Str){
        .p = l->input.p + start,
        .len = len,
        .hash = hash,
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

  if (UNLIKELY(cur(l) != '"')) {
    Str slice = Str_slice(&l->input, l->pos, l->input.len);
    fprintf(stderr, "lex: Unterminated string near: '%.*s'", (int)slice.len,
            slice.p);
    out[count++] = INTERN_EOF;
  } else {
    Token *t = a->request(a->ctx, sizeof(Token));
    t->type = T_STRING;
    t->string = (Str){
        .p = l->input.p + start,
        .len = l->pos - start,
        .hash = hash,
    };
    out[count++] = t;
    // skip "
    l->pos++;
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

unknown: {
  uint8_t c = cur(l);
  ASSERT(0, "Unexpected byte '%c' (0x%X) in input", c, c)
}

end:
  out[count++] = INTERN_EOF;
  return count;
}

#undef SINGLE_TOK
