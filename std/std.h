#include "../vm.h"
#include "fmt.h"
#include "opt.h"
#include "runtime.h"

typedef struct StdNode {
  Str name;
  struct StdNode *children;
  size_t n_children;
  // null if node is a package
  builtin_function fn;
} StdNode;

#define PACKAGE(NAME, ...)                                                     \
  (StdNode) {                                                                  \
    .name = STRING(NAME), .children = (StdNode[]){__VA_ARGS__},                \
    .n_children = sizeof((StdNode[]){__VA_ARGS__}), .fn = NULL                 \
  }

#define FUNCTION(NAME, FN_PTR)                                                 \
  (StdNode) {                                                                  \
    .name = STRING(NAME), .children = NULL, .n_children = 0, .fn = (FN_PTR)    \
  }

void builtin_len(Vm *vm) {
  ASSERT(vm->arg_count == 1, "len only works for a singular argument")
  const Value *a = &ARG(0);
  size_t len = 0;
  if (a->type == V_STR) {
    len = a->string->len;
  } else if (a->type == V_ARRAY) {
    len = a->array->len;
  } else if (a->type == V_OBJ) {
    len = a->obj->entries.len;
  } else {
    fputs("len only strings and lists have a length", stderr);
    exit(EXIT_FAILURE);
  }

  RETURN({
      .type = V_INT,
      .integer = len,
  });
}

// clang-format off
StdNode std_tree = 
    PACKAGE("std",
       PACKAGE("fmt", 
           FUNCTION("println", &builtin_fmt_println),
           FUNCTION("print", &builtin_fmt_print), 
       ), 
       PACKAGE("runtime", 
           FUNCTION("assert", &builtin_runtime_assert),
           FUNCTION("type", &builtin_runtime_type),
       ), 
       FUNCTION("Some", &builtin_opt_some),
       FUNCTION("None", &builtin_opt_none),
       FUNCTION("len", &builtin_len),
   );
// clang-format on
