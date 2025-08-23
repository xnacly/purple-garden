#pragma once

#include "lexer.h"
#include "mem.h"

typedef struct {
  Allocator *alloc;
  Lexer *lexer;
  size_t pos;
  Token *cur;
} Parser;

typedef enum {
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
  // builtins, like @println, @len, @let, @function, etc
  N_BUILTIN,
  // operator, like +-*/%=
  N_BIN,
  // function call
  N_CALL,
  // error and end case
  N_UNKNOWN,
} NodeType;

extern Str NODE_TYPE_MAP[];

// stores all possible values of a node
typedef struct __Node {
  NodeType type;
  // stores the amount of nodes in the
  // functions body or the amount of children in a list
  size_t children_length;
  // stores the children_cap to implement a growing array
  size_t children_cap;
  // N_ATOM values and the N_FUNCTION name are stored in the Token struct - this
  // reduces copies
  Token *token;
  // either children of a list or body of a function, length encoded in
  // Node.children_length
  struct __Node **children;
} Node;

Parser Parser_new(Allocator *alloc, Lexer *l);
Node Parser_next(Parser *p);

#if DEBUG
void Node_debug(Node *n, size_t depth);
#endif
