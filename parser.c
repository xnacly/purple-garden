#include "parser.h"
#include "common.h"
#include "lexer.h"

#define NODE_CAP_GROW 2

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

static bool at_end(Parser *p) { return p->cur.type == T_EOF; }
static void advance(Parser *p) {
  if (!at_end(p)) {
    /*
      #if DEBUG
         Token_debug(&p->cur);
         puts("");
     #endif
    */
    p->cur = Lexer_next(p->lexer);
  }
}

static void consume(Parser *p, TokenType tt) {
  if (p->cur.type != tt) {
    fprintf(stderr, "purple-garden: Unexpected token, wanted: %s, got %s\n",
            String_to(&TOKEN_TYPE_MAP[tt]),
            String_to(&TOKEN_TYPE_MAP[p->cur.type]));
    exit(1);
  }
  advance(p);
}

// attempts to efficiently grow n->children, since lists are the main
// datastructure of purple garden - should be called before each new children
// added to n->children
static void Node_add_child(Node *n, Node child) {
  if (n->children_length + 1 >= n->_children_cap) {
    size_t new_size = n->_children_cap == 0 ? NODE_CAP_GROW
                                            : n->_children_cap * NODE_CAP_GROW;
    n->children = realloc(n->children, new_size * sizeof(Node));
    n->_children_cap = new_size;
  }
  n->children[n->children_length++] = child;
}

static Node parse(Parser *p) {
  switch (p->cur.type) {
  case T_DELIMITOR_LEFT: {
    Node n = (Node){.type = N_LIST, ._children_cap = 0, .children_length = 0};
    consume(p, T_DELIMITOR_LEFT);
    while (!at_end(p) && p->cur.type != T_DELIMITOR_RIGHT) {
      Node children = parse(p);
      Node_add_child(&n, children);
    }
    consume(p, T_DELIMITOR_RIGHT);
    return n;
  }
  case T_STRING:
  case T_BOOLEAN:
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
  default:
    ASSERT(0, "Unimplemented token")
  case T_EOF:
    return (Node){
        .type = N_UNKOWN,
    };
  }
}

void Node_destroy(Node *n) {
  if (n == NULL)
    return;
  for (size_t i = 0; i < n->children_length; i++) {
    Node_destroy(&n->children[i]);
  }
  if (n->children_length > 0) {
    free(n->children);
  }
  Token_destroy(&n->token);
}

#if DEBUG
void Node_debug(Node *n, size_t depth) {
  String NODE_TYPE_MAP[] = {
      // strings, numbers, booleans
      [N_ATOM] = STRING("N_ATOM"),
      //
      [N_IDENT] = STRING("N_IDENT"),
      // main data structure
      [N_LIST] = STRING("N_LIST"),
      // anonymous function
      [N_LAMBDA] = STRING("N_LAMBDA"),
      // operator, like +-*/%
      [N_OP] = STRING("N_OP"),
      // error and end case
      [N_UNKOWN] = STRING("N_UNKOWN"),
  };
  for (size_t i = 0; i < depth; i++) {
    putc(' ', stdout);
  }
  printf("%s", String_to(&NODE_TYPE_MAP[n->type]));
  switch (n->type) {
  case N_ATOM:
  case N_IDENT:
  case N_LAMBDA:
  case N_OP:
    Token_debug(&n->token);
    break;
  case N_LIST:
  case N_UNKOWN:
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
  Node root = (Node){.type = N_LIST, .children_length = 0, ._children_cap = 0};
  while (!at_end(p)) {
    Node n = parse(p);
    if (n.type == N_UNKOWN) {
      break;
    }
    Node_add_child(&root, n);
  }

  return root;
}

#undef SINGLE_NODE
