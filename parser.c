#include "parser.h"
#include "adts.h"
#include "common.h"
#include "lexer.h"
#include "strings.h"

#define NODE_NEW(TYPE, TOKEN)                                                  \
  ({                                                                           \
    Node __n = {0};                                                            \
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
  // Token_debug(p->cur);
  // puts("");
#endif
  p->pos++;
  p->cur = Lexer_next(p->lexer, p->alloc);
}

// TODO: add custom error message here
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

Node Parser_atom(Parser *p) {
  Node n = {0};
  switch (p->cur->type) {
  case T_IDENT:
    n = (Node){.type = N_IDENT, .token = p->cur};
    break;
  case T_DOUBLE:
  case T_INTEGER:
  case T_STRING:
  case T_TRUE:
  case T_FALSE:
    n = (Node){.type = N_ATOM, .token = p->cur};
    break;
  case T_EOF:
  default:
    // TODO: error handling: Wanted an atom, got %q
    ASSERT(0, "cant happen");
    break;
  }
  advance(p);

  return n;
}

Node Parser_builtin(Parser *p) {
  Node builtin = {
      .type = N_BUILTIN,
      .token = p->cur,
      .children = LIST_new(Node),
  };
  consume(p, T_BUILTIN);

  // TODO: has path / namespace, call Parser_path here
  // if (p->cur->type == T_SLASH) {
  // }

  while (p->cur->type != T_EOF && p->cur->type != T_DELIMITOR_RIGHT) {
    Node n = Parser_next(p);
    LIST_append(&builtin.children, p->alloc, n);
  }

  return builtin;
}

// { <key> <value> } both values can be anything we want them to be
Node Parser_obj(Parser *p) {
  Node obj = NODE_NEW(N_OBJECT, p->cur);
  consume(p, T_CURLY_LEFT);

  while (p->cur->type != T_EOF && p->cur->type != T_CURLY_RIGHT) {
    // Key can be anything, i dont care, runtime error, because sometimes we do
    // want dynamic keys, like (+ "user_" name), which would resolve to
    // something like "user_xyz" at runtime, thus setting the corresponding key.
    Node key = Parser_next(p);
    LIST_append(&obj.children, p->alloc, key);
    // Value can also be anything, we are just building a dynamic hashmap like
    // container
    Node val = Parser_next(p);
    LIST_append(&obj.children, p->alloc, val);
  }

  consume(p, T_CURLY_RIGHT);
  return obj;
}

Node Parser_array(Parser *p) {
  consume(p, T_BRAKET_LEFT);

  Node array = (Node){
      .type = N_ARRAY,
      .token = p->cur,
      .children = LIST_new(Node),
  };

  while (p->cur->type != T_EOF && p->cur->type != T_BRAKET_RIGHT) {
    Node n = Parser_next(p);
    LIST_append(&array.children, p->alloc, n);
  }

  consume(p, T_BRAKET_RIGHT);
  return array;
}

// Handles everything inside of s-expr: (<sexpr>)
Node Parser_stmt(Parser *p) {
  consume(p, T_DELIMITOR_LEFT);

  if (p->cur->type == T_DELIMITOR_RIGHT) {
    Node n = NODE_NEW(N_ARRAY, p->cur);
    advance(p);
    return n;
  }

  Node stmt;

  switch (p->cur->type) {
  case T_IDENT:
    stmt = NODE_NEW(N_CALL, p->cur);
    advance(p);
    while (p->cur->type != T_EOF && p->cur->type != T_DELIMITOR_RIGHT) {
      Node n = Parser_next(p);
      LIST_append(&stmt.children, p->alloc, n);
    }
    break;
  case T_PLUS:
  case T_MINUS:
  case T_ASTERISKS:
  case T_SLASH:
  case T_EQUAL:
    stmt = NODE_NEW(N_BIN, p->cur);
    advance(p);
    while (p->cur->type != T_EOF && p->cur->type != T_DELIMITOR_RIGHT) {
      Node n = Parser_next(p);
      LIST_append(&stmt.children, p->alloc, n);
    }
    break;
  case T_BUILTIN:
    stmt = Parser_builtin(p);
    break;
  case T_EOF:
  default:
    // TODO: error handling
    break;
  }

  consume(p, T_DELIMITOR_RIGHT);
  return stmt;
}

Node Parser_next(Parser *p) {
  switch (p->cur->type) {
  case T_IDENT:
  case T_DOUBLE:
  case T_INTEGER:
  case T_STRING:
  case T_TRUE:
  case T_FALSE:
    return Parser_atom(p);
  case T_DELIMITOR_LEFT:
    return Parser_stmt(p);
  case T_BRAKET_LEFT:
    return Parser_array(p);
  case T_CURLY_LEFT:
    return Parser_obj(p);
  case T_EOF:
    Node n = {0};
    n.type = N_UNKNOWN;
    return n;
  default:
    // TODO: error handling:
    ASSERT(0, "EDGE CASE")
  };
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
};

#if DEBUG
void Node_debug(const Node *n, size_t depth) {
  ASSERT(n != NULL, "Node is NULL; THIS SHOULD NEVER HAPPEN");
  for (size_t i = 0; i < depth; i++) {
    putc(' ', stdout);
  }
  if (n->type < 0) {
    Str_debug(&STRING("N_UNKNOWN"));
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
