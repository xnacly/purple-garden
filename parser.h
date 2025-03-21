#ifndef PARSER_H
#define PARSER_H

#include "lexer.h"
#include "list.h"

typedef struct {
  List input;
  size_t pos;
} Parser;

typedef enum {
  N_ATOM,
  N_IDENT,
  N_LIST,
  N_LAMBDA,
} NodeType;

typedef struct Node {
  NodeType type;
  // stores all possible values of a node
  union {
    // used for both string literals and identifier names
    String string;
    double number;
    boolean boolean;
    // params of a lambda, length encoded in Node.param_length
    struct Node *params;
    // either children of a list or body of lambda, length encoded in
    // Node.children_length
    struct Node *children;
  };
  size_t children_length;
  size_t param_length;
} Node;

Parser Parser_new(List token);
// Parser parse the token stream to parse via the Parser into the AST (parse or
// something i dont know)
Node Parser_parse(Parser *p);

#endif
