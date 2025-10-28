#pragma once
#include "../vm.h"

typedef struct StdNode {
  Str name;
  struct StdNode *children;
  size_t len;
  builtin_function fn;
} StdNode;

#define FUNCTION(NAME, FN_PTR)                                                 \
  { .name = STRING(NAME), .children = NULL, .len = 0, .fn = (FN_PTR) }

#define PACKAGE(NAME, ...)                                                     \
  {                                                                            \
    .name = STRING(NAME), .children = (StdNode[]){__VA_ARGS__},                \
    .len = sizeof((StdNode[]){__VA_ARGS__}) / sizeof(StdNode), .fn = NULL      \
  }

StdNode *std_tree(Vm_Config conf);
