#include "parser.h"
#include "common.h"
#include "lexer.h"
#include "strings.h"
#include <stdlib.h>
#include <sys/cdefs.h>

#define NODE_CAP_GROW 1.75
#define NODE_INITIAL_CHILD_SIZE 2

Parser Parser_new(Allocator *alloc, Token *t) {
  return (Parser){
      .alloc = alloc,
      .tokens = t,
      .pos = 0,
      .cur = &t[0],
      .err = false,
  };
}

/* #define advance(P) \
   P->pos++; \ P->cur = &P->tokens[P->pos];
*/

static void advance(Parser *p) {
  p->pos++;
  p->cur = &p->tokens[p->pos];
}

static void consume(Parser *p, TokenType tt) {
  if (UNLIKELY(p->cur->type != tt)) {
    printf("purple-garden: Unexpected token, wanted: ");
    Str_debug(&TOKEN_TYPE_MAP[tt]);
    printf(", got: ");
    Str_debug(&TOKEN_TYPE_MAP[p->cur->type]);
    putc('\n', stdout);
    p->err = true;
    return;
  }
  advance(p);
}

// attempts to efficiently grow n->children, since lists are the main
// datastructure of purple garden - should be called before each new children
// added to n->children
static void Node_add_child(Allocator *alloc, Node *n, Node child) {
  if (n->children_length + 1 >= n->children_cap) {
    // growing array
    size_t new = n->children_cap *NODE_CAP_GROW;
    Node *old = n->children;
    n->children = alloc->request(alloc->ctx, sizeof(Node) * new);
    memcpy(n->children, old, sizeof(Node) * n->children_length);
    n->children_cap = new;
  }

  n->children[n->children_length++] = child;
}

size_t Parser_all(Node *nodes, Parser *p, size_t max_nodes) {
  // stack keeps the list(s) we are in, the last element is always the current
  // list, the first (0) is root
  Node stack[256];
  stack[0] = (Node){
      .type = N_UNKNOWN,
      .children = nodes,
      .children_cap = max_nodes,
  };
  size_t stack_top = 0;

  static void *jump_table[256] = {
      [T_DELIMITOR_LEFT] = &&begin, [T_DELIMITOR_RIGHT] = &&end,
      [T_STRING] = &&atom,          [T_TRUE] = &&atom,
      [T_FALSE] = &&atom,           [T_NUMBER] = &&atom,
      [T_IDENT] = &&ident,          [T_EOF] = &&eof};

#define JUMP_NEXT                                                              \
  do {                                                                         \
    TokenType type = p->cur->type;                                             \
    void *target = jump_table[type];                                           \
    ASSERT(target != NULL, "Unknown token type in parser: '%.*s'",             \
           (int)TOKEN_TYPE_MAP[type].len, TOKEN_TYPE_MAP[type].p);             \
    goto *target;                                                              \
  } while (0)

  JUMP_NEXT;

atom: {
  Node n = (Node){
      .type = N_ATOM,
      .token = p->cur,
  };
  advance(p);
  Node_add_child(p->alloc, &stack[stack_top], n);
  JUMP_NEXT;
}

ident: {
  Node n = (Node){
      .type = N_IDENT,
      .token = p->cur,
  };
  advance(p);
  Node_add_child(p->alloc, &stack[stack_top], n);
  JUMP_NEXT;
}

begin: {
  Node n = (Node){
      .type = N_LIST,
      .children_length = 0,
      .children_cap = NODE_INITIAL_CHILD_SIZE,
      .children = p->alloc->request(p->alloc->ctx,
                                    NODE_INITIAL_CHILD_SIZE * sizeof(Node)),
  };
  consume(p, T_DELIMITOR_LEFT);
  switch (p->cur->type) {
  case T_BUILTIN:
    n.token = p->cur;
    n.type = N_BUILTIN;
    advance(p);
    break;
  case T_PLUS:
  case T_MINUS:
  case T_ASTERISKS:
  case T_SLASH:
    n.token = p->cur;
    n.type = N_OP;
    advance(p);
    break;
  default:
  }
  stack_top++;
  stack[stack_top] = n;
  JUMP_NEXT;
}

end: {
  consume(p, T_DELIMITOR_RIGHT);
  if (stack_top) {
    Node prev = stack[stack_top];
    stack_top--;
    Node_add_child(p->alloc, &stack[stack_top], prev);
  }
  JUMP_NEXT;
}

eof:
  return stack[0].children_length;
}

#if DEBUG
void Node_debug(Node *n, size_t depth) {
  Str NODE_TYPE_MAP[] = {
      // strings, numbers, booleans
      [N_ATOM] = STRING("N_ATOM"),
      //
      [N_IDENT] = STRING("N_IDENT"),
      // main data structure
      [N_LIST] = STRING("N_LIST"),
      // function definition
      [N_FUNCTION] = STRING("N_LAMBDA"),
      // builtin call
      [N_BUILTIN] = STRING("N_BUILTIN"),
      // operator, like +-*/%
      [N_OP] = STRING("N_OP"),
      // error and end case
      [N_UNKNOWN] = STRING("N_UNKOWN"),
  };
  for (size_t i = 0; i < depth; i++) {
    putc(' ', stdout);
  }
  Str_debug(&NODE_TYPE_MAP[n->type]);
  switch (n->type) {
  case N_ATOM:
  case N_IDENT:
  case N_FUNCTION:
  case N_OP:
  case N_BUILTIN:
    Token_debug(n->token);
    break;
  default:
    break;
  }
  if (n->children_length) {
    putc('(', stdout);
    putc('\n', stdout);
  }
  for (size_t i = 0; i < n->children_length; i++) {
    Node_debug(&n->children[i], depth + 1);
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
