#include "parser.h"
#include "common.h"
#include "lexer.h"
#include "strings.h"
#include <stdlib.h>
#include <sys/cdefs.h>

#define NODE_CAP_GROW 2
#define NODE_INITIAL_CHILD_SIZE 8

Parser Parser_new(Allocator *alloc, Token **t) {
  return (Parser){
      .alloc = alloc,
      .tokens = t,
      .pos = 0,
      .cur = t[0],
  };
}

static void advance(Parser *p) {
  p->pos++;
  p->cur = p->tokens[p->pos];
}

static void consume(Parser *p, TokenType tt) {
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

// attempts to efficiently grow n->children, since lists are the main
// datastructure of purple garden - should be called before each new children
// added to n->children
static void Node_add_child(Allocator *alloc, Node *n, Node *child) {
  if (n->children_length + 1 >= n->children_cap) {
    size_t new = n->children_cap *NODE_CAP_GROW;
    new = new < NODE_INITIAL_CHILD_SIZE ? NODE_INITIAL_CHILD_SIZE : new;
    Node **old = n->children;
    n->children = alloc->request(alloc->ctx, sizeof(Node *) * new);
    if (old != NULL) {
      memcpy(n->children, old, sizeof(Node *) * n->children_length);
    }
    n->children_cap = new;
  }

  n->children[n->children_length++] = child;
}

size_t Parser_all(Node **nodes, Parser *p, size_t max_nodes) {
  // stack keeps the list(s) we are in, the last element is always the current
  // list, the first (0) is root
  Node *stack[256] = {0};
  stack[0] = &(Node){
      .type = N_UNKNOWN,
      .children = nodes,
      .children_cap = max_nodes,
  };
  size_t stack_top = 0;

  static void *jump_table[256] = {
      [T_DELIMITOR_LEFT] = &&stmt_begin,
      [T_DELIMITOR_RIGHT] = &&stmt_end,
      [T_BRAKET_LEFT] = &&arr_start,
      [T_BRAKET_RIGHT] = &&arr_end,
      [T_STRING] = &&atom,
      [T_TRUE] = &&atom,
      [T_FALSE] = &&atom,
      [T_DOUBLE] = &&atom,
      [T_INTEGER] = &&atom,
      [T_IDENT] = &&ident,
      [T_EOF] = &&eof,
  };

#define JUMP_NEXT                                                              \
  do {                                                                         \
    void *target = jump_table[p->cur->type];                                   \
    goto *target;                                                              \
  } while (0)

  JUMP_NEXT;

atom: {
  Node *n = p->alloc->request(p->alloc->ctx, sizeof(Node));
  n->type = N_ATOM;
  n->token = p->cur;
  advance(p);
  Node_add_child(p->alloc, stack[stack_top], n);
  JUMP_NEXT;
}

ident: {
  Node *n = p->alloc->request(p->alloc->ctx, sizeof(Node));
  n->type = N_IDENT;
  n->token = p->cur;
  advance(p);
  Node_add_child(p->alloc, stack[stack_top], n);
  JUMP_NEXT;
}

stmt_begin: {
  Node *n = p->alloc->request(p->alloc->ctx, sizeof(Node));
  n->children_length = 0;
  n->children_cap = 0;
  consume(p, T_DELIMITOR_LEFT);
  n->token = p->cur;
  switch (p->cur->type) {
  case T_BUILTIN:
    n->type = N_BUILTIN;
    advance(p);
    break;
  case T_PLUS:
  case T_MINUS:
  case T_ASTERISKS:
  case T_SLASH:
  case T_EQUAL:
    n->type = N_BIN;
    advance(p);
    break;
  case T_IDENT: {
    n->type = N_CALL;
    advance(p);
    break;
  }
  default:
    n->type = N_LIST;
  }
  stack_top++;
  stack[stack_top] = n;
  JUMP_NEXT;
}

stmt_end: {
  consume(p, T_DELIMITOR_RIGHT);
  Node *prev = stack[stack_top];
  stack_top--;
  Node_add_child(p->alloc, stack[stack_top], prev);
  JUMP_NEXT;
}

arr_start: {
  Node *n = p->alloc->request(p->alloc->ctx, sizeof(Node));
  n->children_length = 0;
  n->children_cap = 0;
  n->type = N_ARRAY;
  consume(p, T_BRAKET_LEFT);
  stack_top++;
  stack[stack_top] = n;
  JUMP_NEXT;
}

arr_end: {
  consume(p, T_BRAKET_RIGHT);
  Node *prev = stack[stack_top];
  stack_top--;
  Node_add_child(p->alloc, stack[stack_top], prev);
  JUMP_NEXT;
}

eof:
  ASSERT(!stack_top, "Missing closing delimitor");
  return stack[0]->children_length;
}

Str NODE_TYPE_MAP[] = {
    // strings, numbers, booleans
    [N_ATOM] = STRING("N_ATOM"),
    //
    [N_IDENT] = STRING("N_IDENT"),
    [N_ARRAY] = STRING("N_ARRAY"),
    // main data structure
    [N_LIST] = STRING("N_LIST"),
    // builtin call
    [N_BUILTIN] = STRING("N_BUILTIN"),
    // operator, like +-*/%
    [N_BIN] = STRING("N_BIN"),
    [N_CALL] = STRING("N_CALL"),
    // error and end case
    [N_UNKNOWN] = STRING("N_UNKOWN"),
};

#if DEBUG
void Node_debug(Node *n, size_t depth) {
  for (size_t i = 0; i < depth; i++) {
    putc(' ', stdout);
  }
  Str_debug(&NODE_TYPE_MAP[n->type]);
  switch (n->type) {
  case N_IDENT:
    Token_debug(n->token);
    printf("{hash=%zu}", n->token->string.hash & VARIABLE_TABLE_SIZE_MASK);
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
  if (n->children_length) {
    putc('(', stdout);
    putc('\n', stdout);
  }
  for (size_t i = 0; i < n->children_length; i++) {
    Node_debug(n->children[i], depth + 1);
    if (i + 1 < n->children_length) {
      putc(',', stdout);
    }
    putc('\n', stdout);
  }
  if (n->children_length) {
    for (size_t i = 0; i < depth; i++) {
      putc(' ', stdout);
    }
    putc(')', stdout);
  }
}
#endif
