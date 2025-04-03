#include "parser.h"
#include "common.h"
#include "lexer.h"
#include <stdlib.h>

#define NODE_CAP_GROW 1.75
#define NODE_INITIAL_CHILD_SIZE 8

#define SINGLE_NODE(p, TYPE)                                                   \
  Token t = p->cur;                                                            \
  advance(p);                                                                  \
  return (Node){                                                               \
      .type = TYPE,                                                            \
      .token = t,                                                              \
  };

Parser Parser_new(Lexer *lexer) {
  return (Parser){
      .lexer = lexer,
      .cur = Lexer_next(lexer),
  };
}

static void advance(Parser *p) {
  if (p->cur.type != T_EOF) {
#if DEBUG
    Token_debug(&p->cur);
    puts("");
#endif
    p->cur = Lexer_next(p->lexer);
  }
}

static void consume(Parser *p, TokenType tt) {
  if (p->cur.type != tt) {
    printf("purple-garden: Unexpected token, wanted: ");
    Str_debug(&TOKEN_TYPE_MAP[tt]);
    printf(", got: ");
    Str_debug(&TOKEN_TYPE_MAP[p->cur.type]);
    putc('\n', stdout);
    exit(1);
  }
  advance(p);
}

// attempts to efficiently grow n->children, since lists are the main
// datastructure of purple garden - should be called before each new children
// added to n->children
static void Node_add_child(Node *n, Node child) {
  if (n->children_length + 1 >= n->_children_cap) {
    size_t new_size = n->_children_cap == 0 ? NODE_INITIAL_CHILD_SIZE
                                            : n->_children_cap * NODE_CAP_GROW;
    n->children = realloc(n->children, new_size * sizeof(Node));
    n->_children_cap = new_size;
  }
  n->children[n->children_length++] = child;
}

static Node list_elements(Parser *p) {
  switch (p->cur.type) {
  case T_STRING:
  case T_TRUE:
  case T_FALSE:
  case T_NUMBER: {
    SINGLE_NODE(p, N_ATOM)
  }
  case T_IDENT: {
    SINGLE_NODE(p, N_IDENT)
  }
  case T_PLUS:
  case T_MINUS:
  case T_ASTERISKS:
  case T_SLASH: {
    SINGLE_NODE(p, N_OP)
  }
  case T_AT: {
    advance(p);
    if (p->cur.type == T_IDENT) {
      SINGLE_NODE(p, N_BUILTIN)
    } else {
      TODO("support for other builtins, like objects, here");
    }
  }
  default:
    ASSERT(0, "Unexpected token at this point")
    return (Node){
        .type = N_UNKOWN,
    };
  }
}

static Node parse(Parser *p) {
  switch (p->cur.type) {
  case T_DELIMITOR_LEFT: {
    Node n = (Node){.type = N_LIST, ._children_cap = 0, .children_length = 0};
    consume(p, T_DELIMITOR_LEFT);
    while (p->cur.type != T_EOF && p->cur.type != T_DELIMITOR_RIGHT) {
      Node_add_child(&n, p->cur.type == T_DELIMITOR_LEFT ? parse(p)
                                                         : list_elements(p));
    }
    consume(p, T_DELIMITOR_RIGHT);
    return n;
  }
  case T_STRING:
  case T_TRUE:
  case T_FALSE:
  case T_NUMBER: {
    SINGLE_NODE(p, N_ATOM)
  }
  case T_IDENT: {
    SINGLE_NODE(p, N_IDENT)
  }
  case T_EOF:
    return (Node){
        .type = N_UNKOWN,
    };
  default:
    ASSERT(0, "Unexpected token at this point")
  }
}

void Node_destroy(Node *n) {
  if (n == NULL)
    return;
  for (size_t i = 0; i < n->children_length; i++) {
    Node_destroy(&n->children[i]);
  }
  if (n->_children_cap > 0) {
    free(n->children);
  }
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
      [N_UNKOWN] = STRING("N_UNKOWN"),
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
    Token_debug(&n->token);
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

Node Parser_run(Parser *p) {
  Node root = (Node){.type = N_LIST, .children_length = 0};
  root._children_cap = NODE_INITIAL_CHILD_SIZE * NODE_INITIAL_CHILD_SIZE;
  root.children = malloc(root._children_cap * sizeof(Node));

  while (p->cur.type != T_EOF) {
    Node n = parse(p);
    if (n.type == N_UNKOWN) {
      break;
    }
    Node_add_child(&root, n);
  }

  return root;
}

#undef SINGLE_NODE
