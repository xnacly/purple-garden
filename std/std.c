#include "std.h"
#include "conv.c"
#include "fmt.c"
#include "fs.c"
#include "math.c"
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
    len = a->obj->len;
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
              FUNCTION("print", &builtin_fmt_print, -1), 
              FUNCTION("println", &builtin_fmt_println, -1),
          ), 
          PACKAGE("fs", 
              FUNCTION("read_file", &builtin_fs_read_file, 1), 
              FUNCTION("write_file", &builtin_fs_write_file, 2),
          ), 
          PACKAGE("math", 
              FUNCTION("mod", &builtin_math_mod, 2), 
          ), 
          PACKAGE("opt", 
              FUNCTION("unwrap", &builtin_opt_unwrap, 1), 
              FUNCTION("expect", &builtin_opt_expect, 2),
              FUNCTION("or", &builtin_opt_or, 2), 
              FUNCTION("is_some", &builtin_opt_is_some, 1), 
              FUNCTION("is_none", &builtin_opt_is_none, 1), 
          ), 
          PACKAGE("conv", 
              FUNCTION("int", &builtin_conv_int, 1),
              FUNCTION("num", &builtin_conv_num, 1),
              FUNCTION("str", &builtin_conv_str, 1),
          ), 
          PACKAGE("runtime", 
              FUNCTION("type", &builtin_runtime_type, 1),
              PACKAGE("gc", 
                  FUNCTION("stats", &builtin_runtime_gc_stats, 0)
              ),
          ), 
          FUNCTION("assert", &builtin_runtime_assert, 1),
          FUNCTION("println", &builtin_fmt_println, -1),
          FUNCTION("Some", &builtin_opt_some, 1),
          FUNCTION("None", &builtin_opt_none, 0),
          FUNCTION("len", &builtin_len, 1),
        );

static StdNode reduced = PACKAGE("std", 
          FUNCTION("len", &builtin_len, 1),
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
