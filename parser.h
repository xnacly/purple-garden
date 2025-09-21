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
  // main data structure
  N_LIST,
  // builtins, like @println, @len, @let, @function, etc
  N_BUILTIN,
  // operator, like +-*/%=
  N_BIN,
  // function call
  N_CALL,
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
Node Parser_builtin(Parser *p);
Node Parser_next(Parser *p);
Node Parser_obj(Parser *p);
Node Parser_stmt(Parser *p);

#if DEBUG
void Node_debug(const Node *n, size_t depth);
#endif
