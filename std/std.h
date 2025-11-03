#pragma once
#include "../vm.h"

typedef struct StdNode {
  Str name;
  struct StdNode *children;
  size_t len;
  builtin_function fn;
  int16_t argument_count;
} StdNode;

#define FUNCTION(NAME, FN_PTR, ARG_COUNT)                                      \
  {                                                                            \
    .name = STRING(NAME), .children = NULL, .len = 0, .fn = (FN_PTR),          \
    .argument_count = (ARG_COUNT)                                              \
  }

#define PACKAGE(NAME, ...)                                                     \
  {                                                                            \
    .name = STRING(NAME), .children = (StdNode[]){__VA_ARGS__},                \
    .len = sizeof((StdNode[]){__VA_ARGS__}) / sizeof(StdNode), .fn = NULL,     \
    .argument_count = 0                                                        \
  }

StdNode *std_tree(Vm_Config conf);
