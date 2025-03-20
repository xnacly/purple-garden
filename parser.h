#ifndef PARSER_H
#define PARSER_H

#include "lexer.h"

typedef struct {
  Token *input;
  size_t pos;
  size_t len;
} Parser;

enum NodeType {
  N_ATOM,
  N_IDENT,
  N_LIST,
  N_LAMBDA,
};

typedef struct Node {
  NodeType type;
  union {
    String *string;
    double number;
    // params of a lambda, length encoded in Node.param_length
    Node *params;
    // either children of a list or body of lambda, length encoded in
    // Node.children_length
    Node *children;
  };
  size_t children_length;
  size_t param_length;
} Node;

#endif
