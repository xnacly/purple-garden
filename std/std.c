#include "std.h"
#include "arr.c"
#include "conv.c"
#include "env.c"
#include "fmt.c"
#include "fs.c"
#include "math.c"
#include "opt.c"
#include "runtime.c"
#include "str.c"

static void pg_builtin_len(Vm *vm) {
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
              FUNCTION("print", &pg_builtin_fmt_print, -1), 
              FUNCTION("println", &pg_builtin_fmt_println, -1),
          ), 
          PACKAGE("fs", 
              FUNCTION("read_file", &pg_builtin_fs_read_file, 1), 
              FUNCTION("write_file", &pg_builtin_fs_write_file, 2),
          ), 
          PACKAGE("math", 
              FUNCTION("mod", &pg_builtin_math_mod, 2), 
          ), 
          PACKAGE("str", 
              FUNCTION("append", &pg_builtin_str_append, -1), 
          ), 
          PACKAGE("opt", 
              FUNCTION("unwrap", &pg_builtin_opt_unwrap, 1), 
              FUNCTION("expect", &pg_builtin_opt_expect, 2),
              FUNCTION("or", &pg_builtin_opt_or, 2), 
              FUNCTION("is_some", &pg_builtin_opt_is_some, 1), 
              FUNCTION("is_none", &pg_builtin_opt_is_none, 1), 
          ), 
          PACKAGE("conv", 
              FUNCTION("int", &pg_builtin_conv_int, 1),
              FUNCTION("num", &pg_builtin_conv_num, 1),
              FUNCTION("str", &pg_builtin_conv_str, 1),
          ), 
          PACKAGE("arr", 
              FUNCTION("range", &pg_builtin_arr_range, 2),
              FUNCTION("new", &pg_builtin_arr_new, 1),
          ), 
          PACKAGE("runtime", 
              FUNCTION("type", &pg_builtin_runtime_type, 1),
              PACKAGE("gc", 
                  FUNCTION("stats", &pg_builtin_runtime_gc_stats, 0),
                  FUNCTION("cycle", &pg_builtin_runtime_gc_cycle, 0)
              ),
          ),
          PACKAGE("env", 
              FUNCTION("get", &pg_builtin_env_get, 1),
              FUNCTION("set", &pg_builtin_env_set, 2)
          ),
          FUNCTION("assert", &pg_builtin_runtime_assert, 1),
          FUNCTION("println", &pg_builtin_fmt_println, -1),
          FUNCTION("Some", &pg_builtin_opt_some, 1),
          FUNCTION("None", &pg_builtin_opt_none, 0),
          FUNCTION("len", &pg_builtin_len, 1),
        );

static StdNode reduced = PACKAGE("std", 
          FUNCTION("len", &pg_builtin_len, 1),
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

// std_tree defines the stdlib tree, its packages and functions, computes a hash
// for all nodes and creates other inital states, like the env
StdNode *std_tree(Vm_Config conf, Allocator *a) {
  StdNode *selected = &tree;
  if (conf.disable_std) {
    selected = &reduced;
  } else {
    setup_env(conf, a);
  }

  compute_hashes(selected);
  return selected;
}
