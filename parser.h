#pragma once

#include "lexer.h"
#include "mem.h"

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
  // path to a namespace, like std/fmt/println or std/os/env/USER
  N_PATH,
} NodeType;

extern Str NODE_TYPE_MAP[];

typedef struct Node Node;
typedef Node *Nptr;

LIST_TYPE(Nptr);

// stores all possible values of a node
typedef struct Node {
  NodeType type;
  // N_ATOM values and the N_FUNCTION name are stored in the Token struct - this
  // reduces copies
  Token *token;
  LIST_Nptr children;
} Node;

typedef struct Parser {
  Allocator *alloc;
  Lexer *lexer;
  size_t pos;
  Token *cur;
} Parser;

Parser Parser_new(Allocator *alloc, Lexer *l);
Node *Parser_next(Parser *p);
void Node_debug(const Node *n, size_t depth);
