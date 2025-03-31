#ifndef PARSER_H
#define PARSER_H

#include "lexer.h"

typedef struct {
  Lexer *lexer;
  Token cur;
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
  // builtins, like println
  N_BUILTIN,
  // operator, like +-*/%
  N_OP,
  // error and end case
  N_UNKOWN,
} NodeType;

// stores all possible values of a node
typedef struct Node {
  NodeType type;
  // N_ATOM values and the N_FUNCTION name are stored in the Token struct - this
  // reduces copies
  Token token;
  // params of a lambda, length encoded in Node.param_length
  struct Node *params;
  // either children of a list or body of lambda, length encoded in
  // Node.children_length
  struct Node *children;
  // only populated for N_LAMBDA and N_LIST; stores the amount of nodes in the
  // lambdas body or the amount of children in a list
  size_t children_length;
  // only populated for N_LAMBDA; stores the lambda parameter count
  size_t param_length;
  // private field for efficient allocation of children
  size_t _children_cap;
} Node;

Parser Parser_new(Lexer *lexer);
// Returns the root of a file as a Node of type N_LIST, contains all nodes in
// said file as children
Node Parser_run(Parser *p);
// Deallocates a node and all its children by calling itself on each one
void Node_destroy(Node *n);
void Node_debug(Node *n, size_t depth);

#endif
