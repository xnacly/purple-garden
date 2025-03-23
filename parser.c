#include "parser.h"
#include "common.h"
#include "lexer.h"

Parser Parser_new(Lexer *lexer) {
  return (Parser){
      .lexer = lexer,
      .cur = Lexer_next(lexer),
  };
}

static boolean at_end(Parser *p) { return p->cur.type == T_EOF; }
static void advance(Parser *p) {
  if (!at_end(p)) {
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

static Node list(Parser *p) {
  consume(p, T_DELIMITOR_LEFT);
  consume(p, T_DELIMITOR_RIGHT);
  return (Node){};
}

static Node atom(Parser *p) {
  switch (p->cur.type) {
  case T_STRING:
  case T_BOOLEAN:
  case T_NUMBER:
  case T_IDENT:
    break;
  default:
    ASSERT(0, "Wanted T_STRING, T_BOOLEAN, T_NUMBER, T_IDENT, did not get it")
  }
  Token t = p->cur;
  advance(p);
  return (Node){
      .type = N_ATOM,
      .token = t,
  };
}

static Node parse(Parser *p) {
  switch (p->cur.type) {
  case T_DELIMITOR_LEFT:
    return list(p);
  case T_STRING:
  case T_BOOLEAN:
  case T_NUMBER:
  case T_IDENT:
    return atom(p);
  default:
  case T_EOF:
    return (Node){
        .type = N_UNKOWN,
    };
  }
  advance(p);
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

Node Parser_run(Parser *p) {
  Node root = (Node){.type = N_LIST, .children_length = 0};
  while (!at_end(p)) {
    Node n = parse(p);
    if (n.type != N_UNKOWN) {
      // TODO: Do reallactions with a factor of *2, not for each node - this
      // will be slow due to allocations in hot paths - abstract it into
      // functions called static node_list and static node_list_grow; also check
      // if allocation was successful
      root.children =
          realloc(root.children,
                  (root.children_length == 0 ? 1 : root.children_length + 1) *
                      sizeof(Node));
      root.children[root.children_length++] = n;
    }
  }

  return root;
}
