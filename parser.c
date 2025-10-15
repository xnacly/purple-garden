#include "parser.h"
#include "adts.h"
#include "common.h"
#include "lexer.h"
#include "strings.h"

#define TRACE_PARSER 0
#define EQUALS(TYPE) p->cur->type == (TYPE)

#if TRACE_PARSER
static size_t call_depth = 0;
#define TRACE(FUNC)                                                            \
  ({                                                                           \
    printf("%*s->" #FUNC "#%.*s\n", (int)call_depth, "",                       \
           (int)TOKEN_TYPE_MAP[p->cur->type].len,                              \
           TOKEN_TYPE_MAP[p->cur->type].p);                                    \
    call_depth++;                                                              \
    Node __n = (FUNC)(p);                                                      \
    call_depth--;                                                              \
    __n;                                                                       \
  })
#else
#define TRACE(FUNC) FUNC(p)
#endif

#define NODE_NEW(TYPE, TOKEN)                                                  \
  ({                                                                           \
    Node *__n = CALL(p->alloc, request, sizeof(Node));                         \
    __n->type = TYPE;                                                          \
    __n->token = TOKEN;                                                        \
    __n->children = LIST_new(Nptr);                                            \
    __n;                                                                       \
  })

Str NODE_TYPE_MAP[] = {
    [N_ATOM] = STRING("N_ATOM"),   [N_IDENT] = STRING("N_IDENT"),
    [N_ARRAY] = STRING("N_ARRAY"), [N_OBJECT] = STRING("N_OBJECT"),
    [N_LIST] = STRING("N_LIST"),   [N_VAR] = STRING("N_VAR"),
    [N_FN] = STRING("N_FN"),       [N_MATCH] = STRING("N_MATCH"),
    [N_BIN] = STRING("N_BIN"),     [N_CALL] = STRING("N_CALL"),
    [N_PATH] = STRING("N_PATH"),
};

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
  case N_FN:
    printf("[");
    Str_debug(&n->token->string);
    printf("]");
    break;
  case N_VAR:
  case N_ATOM:
  case N_BIN:
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
    Node *ni = LIST_get(&n->children, i);
    Node_debug(ni, depth + 1);
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

static inline Token *consume(Parser *p, TokenType type) {
  Token *t = p->cur;
  if (UNLIKELY(t->type != type)) {
    printf("Unexpected Token %.*s, wanted %.*s\n",
           (int)TOKEN_TYPE_MAP[t->type].len, TOKEN_TYPE_MAP[t->type].p,
           (int)TOKEN_TYPE_MAP[(type)].len, TOKEN_TYPE_MAP[(type)].p);
    ASSERT(0, "Unexpected Token");
  }
  advance(p);
  return t;
}

static Node *Parser_expr(Parser *p);
static Node *Parser_term(Parser *p);
static Node *Parser_comparison(Parser *p);
static Node *Parser_atom(Parser *p);
static Node *Parser_obj(Parser *p);
static Node *Parser_array(Parser *p);

inline static Node *Parser_expr(Parser *p) {
  Node *lhs = TRACE(Parser_term);
  while (p->cur->type == T_PLUS || p->cur->type == T_MINUS) {
    Token *op = p->cur;
    advance(p);
    Node *rhs = TRACE(Parser_term);
    Node *bin = NODE_NEW(N_BIN, op);
    LIST_append(&bin->children, p->alloc, lhs);
    LIST_append(&bin->children, p->alloc, rhs);
    lhs = bin;
  }
  return lhs;
}

inline static Node *Parser_term(Parser *p) {
  Node *lhs = TRACE(Parser_atom);
  while (p->cur->type == T_ASTERISKS || p->cur->type == T_SLASH) {
    Token *op = p->cur;
    advance(p);
    Node *rhs = TRACE(Parser_atom);
    Node *bin = NODE_NEW(N_BIN, op);
    LIST_append(&bin->children, p->alloc, lhs);
    LIST_append(&bin->children, p->alloc, rhs);
    lhs = bin;
  }
  return lhs;
}

inline static Node *Parser_comparison(Parser *p) {
  Node *lhs = TRACE(Parser_expr);
  while (p->cur->type == T_EQUAL) {
    Token *op = p->cur;
    advance(p);
    Node *rhs = TRACE(Parser_expr);
    Node *bin = NODE_NEW(N_BIN, op);
    LIST_append(&bin->children, p->alloc, lhs);
    LIST_append(&bin->children, p->alloc, rhs);
    lhs = bin;
  }
  return lhs;
}

inline static Node *Parser_atom(Parser *p) {
  switch (p->cur->type) {
  case T_INTEGER:
  case T_DOUBLE:
  case T_STRING:
  case T_TRUE:
  case T_FALSE: {
    Node *n = CALL(p->alloc, request, sizeof(Node));
    n->type = N_ATOM;
    n->token = p->cur;
    advance(p);
    return n;
  }
  case T_IDENT: { // variable name or <name>(<args>)
    Token *ident = p->cur;
    advance(p);
    // TODO: i gotta work on this one
    // if (EQUALS(T_SLASH)) {
    //   Node path = NODE_NEW(N_PATH, ident);
    //   consume(p, T_SLASH);
    //   while (p->cur->type != T_EOF) {
    //     if (p->cur->type == T_IDENT) {
    //       Node path_ident = (Node){.type = N_IDENT, .token = p->cur};
    //       LIST_append(&path.children, p->alloc, path_ident);
    //       advance(p);
    //     } else {
    //       // TODO: error handling
    //       ASSERT(0, "Wanted IDENT in path")
    //     }

    //     if (p->cur->type != T_SLASH) {
    //       break;
    //     }
    //   }

    //   return path;
    // }

    if (EQUALS(T_DELIMITOR_LEFT)) {
      consume(p, T_DELIMITOR_LEFT);
      // PERF: perform builtin function / user function lookup here once and
      // cache, instead of doing it a whole lot in the compiler
      Node *call = NODE_NEW(N_CALL, ident);
      while (p->cur->type != T_EOF && p->cur->type != T_DELIMITOR_RIGHT) {
        Node *n = TRACE(Parser_next);
        LIST_append(&call->children, p->alloc, n);
      }
      consume(p, T_DELIMITOR_RIGHT);
      return call;
    } else {

      Node *n = CALL(p->alloc, request, sizeof(Node));
      n->type = N_IDENT;
      n->token = ident;
      return n;
    }
  }
  case T_DELIMITOR_LEFT: {
    consume(p, T_DELIMITOR_LEFT);
    Node *expr = TRACE(Parser_expr);
    consume(p, T_DELIMITOR_RIGHT);
    return expr;
  }
  case T_EOF:
  default:
    __builtin_unreachable();
  }
}

// { <key> <value> } both values can be anything we want them to be
inline static Node *Parser_obj(Parser *p) {
  Node *obj = NODE_NEW(N_OBJECT, p->cur);
  consume(p, T_CURLY_LEFT);

  while (p->cur->type != T_EOF && p->cur->type != T_CURLY_RIGHT) {
    // Key can be anything, i dont care, runtime error, because sometimes we do
    // want dynamic keys, like (+ "user_" name), which would resolve to
    // something like "user_xyz" at runtime, thus setting the corresponding key.
    Node *key = TRACE(Parser_next);
    LIST_append(&obj->children, p->alloc, key);
    // Value can also be anything, we are just building a dynamic hashmap like
    // container
    Node *val = TRACE(Parser_next);
    LIST_append(&obj->children, p->alloc, val);
  }

  consume(p, T_CURLY_RIGHT);
  return obj;
}

inline static Node *Parser_array(Parser *p) {
  Node *array = NODE_NEW(N_ARRAY, p->cur);
  consume(p, T_BRAKET_LEFT);

  while (p->cur->type != T_EOF && p->cur->type != T_BRAKET_RIGHT) {
    Node *n = TRACE(Parser_next);
    LIST_append(&array->children, p->alloc, n);
  }

  consume(p, T_BRAKET_RIGHT);
  return array;
}

Node *Parser_next(Parser *p) {
  switch (p->cur->type) {
  case T_BRAKET_LEFT:
    return TRACE(Parser_array);
  case T_CURLY_LEFT:
    return TRACE(Parser_obj);
  case T_VAR: { // var <ident> <rhs>
    consume(p, T_VAR);
    Token *ident = consume(p, T_IDENT);
    consume(p, T_DOUBLEDOUBLEDOT);
    Node *var = NODE_NEW(N_VAR, ident);
    Node *rhs = TRACE(Parser_next);
    LIST_append(&var->children, p->alloc, rhs);
    return var;
  }
  case T_MATCH: {
    TODO("Unimplemented");
  }
  case T_FN: { // fn <name>(<args>){ <body> }
    consume(p, T_FN);
    Token *ident = consume(p, T_IDENT);
    Node *fn = NODE_NEW(N_FN, ident);

    Node *params = NODE_NEW(N_ARRAY, p->cur);
    consume(p, T_DOUBLEDOUBLEDOT);
    while (p->cur->type != T_EOF && p->cur->type != T_CURLY_LEFT) {
      Node *n = TRACE(Parser_next);
      LIST_append(&params->children, p->alloc, n);
    }
    LIST_append(&fn->children, p->alloc, params);

    consume(p, T_CURLY_LEFT);
    while (p->cur->type != T_EOF && p->cur->type != T_CURLY_RIGHT) {
      Node *body_part = TRACE(Parser_next);
      LIST_append(&fn->children, p->alloc, body_part);
    }
    consume(p, T_CURLY_RIGHT);
    return fn;
  }
  case T_EOF: {
    return NULL;
  }
  default: {
    return TRACE(Parser_comparison);
    // TODO: error handling:
    // ASSERT(0,
    //        "Unexpected Token %.*s, wanted any of: IDENT, DOUBLE, "
    //        "INTEGER, STRING, TRUE, FALSE, DELIMITOR_LEFT, "
    //        "BRAKELEFT, CURLY_LEFT",
    //        (int)TOKEN_TYPE_MAP[p->cur->type].len,
    //        TOKEN_TYPE_MAP[p->cur->type].p)
  }
  };
}
