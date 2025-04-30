#include "lexer.h"
#include "common.h"
#include "mem.h"
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

static bool is_alphanum(char cc) {
  return (cc >= 'a' && cc <= 'z') || (cc >= 'A' && cc <= 'Z') ||
         (cc >= '0' && cc <= '9') || cc == '_' || cc == '-';
}

// we can "intern" these, since all of them are the same, regardless of position
Token *INTERN_DELIMITOR_LEFT = &(Token){.type = T_DELIMITOR_LEFT};
Token *INTERN_DELIMITOR_RIGHT = &(Token){.type = T_DELIMITOR_RIGHT};
Token *INTERN_BRAKET_LEFT = &(Token){.type = T_BRAKET_LEFT};
Token *INTERN_BRAKET_RIGHT = &(Token){.type = T_BRAKET_RIGHT};
Token *INTERN_MINUS = &(Token){.type = T_MINUS};
Token *INTERN_PLUS = &(Token){.type = T_PLUS};
Token *INTERN_ASTERISKS = &(Token){.type = T_ASTERISKS};
Token *INTERN_SLASH = &(Token){.type = T_SLASH};
Token *INTERN_FALSE = &(Token){.type = T_FALSE};
Token *INTERN_TRUE = &(Token){.type = T_TRUE};
Token *INTERN_EOF = &(Token){.type = T_EOF};

size_t Lexer_all(Lexer *l, Allocator *a, Token **out) {
  ASSERT(out != NULL, "Failed to allocate token list");
  size_t count = 0;
  static void *jump_table[256] = {
      ['('] = &&delimitor_left, [')'] = &&delimitor_right,
      ['['] = &&braket_left,    [']'] = &&braket_right,
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
    ASSERT(!(c & 0x80), "Non-ASCII input character!");                         \
    void *target = jump_table[c];                                              \
    ASSERT(target != NULL, "Unknown character in lexer: '%c'(%d)",             \
           l->input.p[l->pos], l->input.p[l->pos]);                            \
    goto *target;                                                              \
  } while (0);

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

asterisks:
  out[count++] = INTERN_ASTERISKS;
  l->pos++;
  JUMP_TARGET;

number: {
  size_t start = l->pos;
  const char *input_start = l->input.p + start;
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

  Token *n = a->request(a->ctx, sizeof(Token));
  if (is_double) {
    n->type = T_DOUBLE;
    n->floating = strtod(input_start, &endptr);
  } else {
    n->type = T_INTEGER;
    n->integer = strtol(input_start, &endptr, 10);
  }

  ASSERT(endptr != input_start, "lex: Failed to parse number")
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
  if (len == 4 &&
      (l->input.p[start + 0] == 't' && l->input.p[start + 1] == 'r' &&
       l->input.p[start + 2] == 'u' && l->input.p[start + 3] == 'e')) {
    t = INTERN_TRUE;
  } else if (len == 5 &&
             (l->input.p[start + 0] == 'f' && l->input.p[start + 1] == 'a' &&
              l->input.p[start + 2] == 'l' && l->input.p[start + 3] == 's' &&
              l->input.p[start + 4] == 'e')) {
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

  if (cur(l) != '"') {
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

end:
  out[count++] = INTERN_EOF;
  return count;
}

#undef SINGLE_TOK
