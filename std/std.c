#include "std.h"
#include "fmt.c"
#include "opt.c"
#include "runtime.c"

static void builtin_len(Vm *vm) {
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
static StdNode tree = PACKAGE("std",
          PACKAGE("fmt", 
              FUNCTION("print", &builtin_fmt_print), 
              FUNCTION("println", &builtin_fmt_println),
          ), 
          PACKAGE("runtime", 
              FUNCTION("type", &builtin_runtime_type),
              PACKAGE("gc", 
                  FUNCTION("stats", &builtin_runtime_gc_stats)
              ),
          ), 
          FUNCTION("assert", &builtin_runtime_assert),
          FUNCTION("println", &builtin_fmt_println),
          FUNCTION("Some", &builtin_opt_some),
          FUNCTION("None", &builtin_opt_none),
          FUNCTION("len", &builtin_len),
        );

static StdNode reduced = PACKAGE("std", 
          FUNCTION("Some", &builtin_opt_some),
          FUNCTION("None", &builtin_opt_none),
          FUNCTION("len", &builtin_len),
        );
// clang-format on

static void compute_hashes(StdNode *node) {
  if (!node)
    return;

  node->name.hash = Str_hash(&node->name);

  for (size_t i = 0; i < node->len; i++) {
    compute_hashes(&node->children[i]);
  }
}

StdNode *std_tree(Vm_Config conf) {
  StdNode *selected = &tree;
  if (conf.disable_std) {
    selected = &reduced;
  }
  compute_hashes(selected);
  return selected;
}
