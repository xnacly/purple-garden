#ifndef PARSER_H
#define PARSER_H

#include "lexer.h"
#include "mem.h"

typedef struct {
  Allocator *alloc;
  Token **tokens;
  size_t pos;
  Token *cur;
} Parser;

typedef enum {
  // strings, numbers, booleans
  N_ATOM,
  //
  N_IDENT,
  // main data structure
  N_LIST,
  // function definition
  N_FUNCTION,
  // builtins, like @println, @len, etc
  N_BUILTIN,
  // operator, like +-*/%
  N_OP,
  // error and end case
  N_UNKNOWN,
} NodeType;

// stores all possible values of a node
typedef struct Node {
  NodeType type;
  // only populated for N_LAMBDA and N_LIST; stores the amount of nodes in the
  // lambdas body or the amount of children in a list
  size_t children_length;
  // only populated for N_LAMBDA; stores the lambda parameter count
  size_t param_length;
  // stores the children_cap to implement a growing array
  size_t children_cap;
  // N_ATOM values and the N_FUNCTION name are stored in the Token struct - this
  // reduces copies
  Token *token;
  // params of a lambda, length encoded in Node.param_length
  struct Node **params;
  // either children of a list or body of lambda, length encoded in
  // Node.children_length
  struct Node **children;
} Node;

Parser Parser_new(Allocator *alloc, Token **t);
// Returns the next top level Node
Node Parser_next(Parser *p);
size_t Parser_all(Node **nodes, Parser *p, size_t max_nodes);
void Node_debug(Node *n, size_t depth);

#endif
