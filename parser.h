#pragma once

#include "lexer.h"
#include "mem.h"

#if DEBUG
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
    Node __n = {0};                                                            \
    __n.type = TYPE;                                                           \
    __n.token = TOKEN;                                                         \
    __n.children = LIST_new(Node);                                             \
    __n;                                                                       \
  })

typedef enum {
  // error case
  N_UNKNOWN = -1,
  // strings, numbers, booleans
  N_ATOM,
  // all identifiers
  N_IDENT,
  // anything between [ and ]
  N_ARRAY,
  // anything between { and }
  N_OBJECT,
  // main data structure
  N_LIST,
  // creating variables
  N_VAR,
  // defining functions
  N_FN,
  // conditional logic
  N_MATCH,
  // operator, like +-*/%=
  N_BIN,
  // function call
  N_CALL,
  // path to a namespace, object or array index, like std/fmt/println or
  // std/os/env/USER
  N_PATH,
} NodeType;

extern Str NODE_TYPE_MAP[];

typedef struct Node Node;

LIST_TYPE(Node);

// stores all possible values of a node
typedef struct Node {
  NodeType type;
  // N_ATOM values and the N_FUNCTION name are stored in the Token struct - this
  // reduces copies
  Token *token;
  LIST_Node children;
} Node;

typedef struct Parser Parser;
typedef struct Parser {
  Allocator *alloc;
  Lexer *lexer;
  size_t pos;
  Token *cur;
} Parser;

Parser Parser_new(Allocator *alloc, Lexer *l);
Node Parser_next(Parser *p);

// necessary for recursive descent parser
Node Parser_array(Parser *p);
Node Parser_atom(Parser *p);
Node Parser_next(Parser *p);
Node Parser_obj(Parser *p);
Node Parser_stmt(Parser *p);

#if DEBUG
void Node_debug(const Node *n, size_t depth);
#endif
