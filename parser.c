#include "parser.h"
#include "adts.h"
#include "common.h"
#include "lexer.h"
#include "strings.h"
#include <stdlib.h>

#define NODE_NEW(TYPE, TOKEN)                                                  \
  ({                                                                           \
    Node __n;                                                                  \
    __n.type = TYPE;                                                           \
    __n.token = TOKEN;                                                         \
    __n.children = LIST_new(Node);                                             \
    __n;                                                                       \
  })

Parser Parser_new(Allocator *alloc, Lexer *l) {
  return (Parser){
      .alloc = alloc,
      .lexer = l,
      .pos = 0,
      .cur = Lexer_next(l, alloc),
  };
}

static inline void advance(Parser *p) {
#if DEBUG
  Token_debug(p->cur);
  puts("");
#endif
  p->pos++;
  p->cur = Lexer_next(p->lexer, p->alloc);
}

static inline void consume(Parser *p, TokenType tt) {
  if (UNLIKELY(p->cur->type != tt)) {
    printf("purple-garden: Unexpected token, wanted: ");
    Str_debug(&TOKEN_TYPE_MAP[tt]);
    printf(", got: ");
    Str_debug(&TOKEN_TYPE_MAP[p->cur->type]);
    putc('\n', stdout);
    return;
  }

  advance(p);
}

Node Parser_next(Parser *p) {
#define MAX_DEPTH 256
  // stack keeps the list(s) we are in, the last element is always the current
  // list, the first (0) is root
  Node stack[MAX_DEPTH];
  stack[0] = (Node){
      .type = N_ROOT,
      .children = LIST_new(Node),
  };
  size_t stack_top = 0;

#pragma GCC diagnostic push
  // We know what we're doing, so this is fine:
  //
  // we assign unknown to all and overwrite these to make sure an invalid
  // index is not a unassigned memory access.
#pragma GCC diagnostic ignored "-Woverride-init"
  static void *jump_table[256] = {
      [0 ... 255] = &&unkown,
      [T_DELIMITOR_LEFT] = &&stmt_begin,
      [T_DELIMITOR_RIGHT] = &&stmt_end,
      [T_BRAKET_LEFT] = &&arr_start,
      [T_BRAKET_RIGHT] = &&arr_end,
      [T_CURLY_LEFT] = &&obj_start,
      [T_CURLY_RIGHT] = &&obj_end,
      [T_STRING] = &&atom,
      [T_TRUE] = &&atom,
      [T_FALSE] = &&atom,
      [T_DOUBLE] = &&atom,
      [T_INTEGER] = &&atom,
      [T_IDENT] = &&ident,
      [T_EOF] = &&eof,
  };
#pragma GCC diagnostic pop

#define JUMP_NEXT goto *jump_table[p->cur->type];

  ASSERT(stack_top < MAX_DEPTH, "Stack overflow, max 256 stack depth");
  JUMP_NEXT;

atom: {
  Node n = NODE_NEW(N_ATOM, p->cur);
  advance(p);
  LIST_append(&stack[stack_top].children, p->alloc, n);
  JUMP_NEXT;
}

ident: {
  Node n = NODE_NEW(N_IDENT, p->cur);
  advance(p);
  LIST_append(&stack[stack_top].children, p->alloc, n);
  JUMP_NEXT;
}

stmt_begin: {
  Node n = NODE_NEW(N_UNKNOWN, p->cur);
  consume(p, T_DELIMITOR_LEFT);
  n.token = p->cur;
  switch (p->cur->type) {
  case T_BUILTIN:
    n.type = N_BUILTIN;
    advance(p);
    break;
  case T_PLUS:
  case T_MINUS:
  case T_ASTERISKS:
  case T_SLASH:
  case T_EQUAL:
    n.type = N_BIN;
    advance(p);
    break;
  case T_IDENT: {
    n.type = N_CALL;
    advance(p);
    break;
  }
  default:
    n.type = N_LIST;
  }
  stack_top++;
  stack[stack_top] = n;
  JUMP_NEXT;
}

stmt_end: {
  ASSERT(stack_top != 0, "Unexpected expr end");
  consume(p, T_DELIMITOR_RIGHT);
  Node prev = stack[stack_top];
  stack_top--;
  LIST_append(&stack[stack_top].children, p->alloc, prev);
  // I think we should stop this here if stack_top == 0, right? Thus stopping at
  // a root parse
  if (stack_top == 0) {
    return LIST_get_UNSAFE(&stack[0].children, 0);
  } else {
    JUMP_NEXT
  }
}

arr_start: {
  Node n = NODE_NEW(N_ARRAY, p->cur);
  consume(p, T_BRAKET_LEFT);
  stack_top++;
  stack[stack_top] = n;
  JUMP_NEXT;
}

arr_end: {
  ASSERT(stack_top != 0, "Unexpected array end");
  consume(p, T_BRAKET_RIGHT);
  Node prev = stack[stack_top];
  stack_top--;
  LIST_append(&stack[stack_top].children, p->alloc, prev);
  JUMP_NEXT;
}

obj_start: {
  Node n = NODE_NEW(N_OBJECT, p->cur);
  consume(p, T_CURLY_LEFT);
  stack_top++;
  stack[stack_top] = n;
  JUMP_NEXT;
}

obj_end: {
  ASSERT(stack_top != 0, "Unexpected obj end");
  consume(p, T_CURLY_RIGHT);
  Node prev = stack[stack_top];
  stack_top--;
  LIST_append(&stack[stack_top].children, p->alloc, prev);
  JUMP_NEXT;
}

eof: // we dont have any more input, return stack top
  Node n = stack[stack_top];
  ASSERT(n.children.len == 0, "Top level sexpr necessary");
  return n;

// we want to error here
unkown:
  return (Node){.type = N_UNKNOWN};
}

Str NODE_TYPE_MAP[] = {
    // strings, numbers, booleans
    [N_ATOM] = STRING("N_ATOM"),
    //
    [N_IDENT] = STRING("N_IDENT"),
    [N_ARRAY] = STRING("N_ARRAY"),
    [N_OBJECT] = STRING("N_OBJECT"),
    // main data structure
    [N_LIST] = STRING("N_LIST"),
    // builtin call
    [N_BUILTIN] = STRING("N_BUILTIN"),
    // operator, like +-*/%
    [N_BIN] = STRING("N_BIN"),
    [N_CALL] = STRING("N_CALL"),
    // error and end case
    [N_ROOT] = STRING("N_ROOT"),
};

#if DEBUG
void Node_debug(const Node *n, size_t depth) {
  ASSERT(n != NULL, "Node is NULL; THIS SHOULD NEVER HAPPEN");
  for (size_t i = 0; i < depth; i++) {
    putc(' ', stdout);
  }
  if (n->type < 0) {
    Str_debug(&STRING("N_UNKOWN"));
  } else {
    Str_debug(&NODE_TYPE_MAP[n->type]);
  }
  switch (n->type) {
  case N_IDENT:
    Token_debug(n->token);
    printf("{idx=%lu}", n->token->string.hash & VARIABLE_TABLE_SIZE_MASK);
    break;
  case N_ATOM:
  case N_BIN:
  case N_BUILTIN:
    Token_debug(n->token);
    break;
  case N_CALL:
    putc('[', stdout);
    Str_debug(&n->token->string);
    putc(']', stdout);
    break;
  default:
    break;
  }
  if (n->children.len) {
    putc('(', stdout);
    putc('\n', stdout);
  }
  for (size_t i = 0; i < n->children.len; i++) {
    Node ni = LIST_get(&n->children, i);
    Node_debug(&ni, depth + 1);
    if (i + 1 < n->children.len) {
      putc(',', stdout);
    }
    putc('\n', stdout);
  }
  if (n->children.len) {
    for (size_t i = 0; i < depth; i++) {
      putc(' ', stdout);
    }
    putc(')', stdout);
  }
}
#endif
