#include "assert.h"
#include <stdlib.h>
#include <string.h>

#include "cc.h"
#include "common.h"
#include "lexer.h"
#include "mem.h"
#include "parser.h"
#include "strings.h"
#include "vm.h"

#if DEBUG
#define DEBUG_PUTS(fmt, ...)                                                   \
  do {                                                                         \
    printf("[CC] " fmt "\n", ##__VA_ARGS__);                                   \
  } while (0)
#else
#define DEBUG_PUTS(fmt, ...)
#endif

// token_to_value converts tokens, such as strings, boolean and numbers to
// runtime values
inline static Value *token_to_value(Token *t, Allocator *a) {
  Value *v = CALL(a, request, sizeof(Value));
  switch (t->type) {
  case T_STRING:
  case T_IDENT:
    v->type = V_STR;
    v->string = t->string;
    break;
  case T_TRUE:
    v->type = V_TRUE;
    break;
  case T_FALSE:
    v->type = V_FALSE;
    break;
  case T_INTEGER:
    v->type = V_INT;
    v->integer = Str_to_int64_t(&t->string);
    break;
  case T_DOUBLE:
    v->type = V_DOUBLE;
    v->floating = Str_to_double(&t->string);
    break;
  default:
    ASSERT(0, "Unsupported value for token_to_value");
    break;
  }
  return v;
}

static size_t Ctx_allocate_register(Ctx *ctx) {
  ASSERT(ctx->register_allocated_count < REGISTERS, "cc: out of registers")
  ctx->registers[ctx->register_allocated_count] = true;
#if DEBUG
  printf("allocating r%zu\n", ctx->register_allocated_count);
#endif
  return ctx->register_allocated_count++;
}

static void Ctx_free_register(Ctx *ctx, size_t i) {
  assert(i < ctx->register_allocated_count &&
         "cc: register index out of bounds");
  assert(ctx->registers[i] && "cc: attempting to free unallocated register");
  ctx->register_allocated_count--;
#if DEBUG
  printf("freeing r%zu\n", ctx->register_allocated_count);
#endif
  ctx->registers[i] = false;
}

static size_t runtime_builtin_hashes[MAX_BUILTIN_SIZE + 1];

static void compile(Allocator *alloc, Vm *vm, Ctx *ctx, Node *n) {
  switch (n->type) {
  case N_ATOM: {
    // interning logic, global pool 0 is the only instance for false in the
    // runtime, 1 for true, strings get interned by their hashes, doubles by
    // their bits and ints by their integer representation

// High tag bits
#define TAG_FALSE 0x1000000000000000ULL
#define TAG_TRUE 0x2000000000000000ULL
#define TAG_STRING 0x3000000000000000ULL
#define TAG_DOUBLE 0x4000000000000000ULL
#define TAG_INT 0x5000000000000000ULL

// Tag mask to keep lower bits
#define TAG_MASK 0x0FFFFFFFFFFFFFFFULL

    size_t hash;
    if (n->token->type == T_FALSE) {
      BC(OP_LOADG, GLOBAL_FALSE);
      break;
    } else if (n->token->type == T_TRUE) {
      BC(OP_LOADG, GLOBAL_TRUE);
      break;
    } else if (n->token->type == T_STRING) {
      hash = TAG_STRING | (n->token->string.hash & TAG_MASK);
    } else if (n->token->type == T_DOUBLE) {
      // type punning by using token->integer while token->floating is filled
      hash = TAG_DOUBLE | (n->token->string.hash & TAG_MASK);
    } else if (n->token->type == T_INTEGER) {
      hash = TAG_INT | (n->token->string.hash & TAG_MASK);
    } else {
      ASSERT(vm->global_len + 1 < GLOBAL_SIZE,
             "cc: out of global space, what the fuck are you doing");
      vm->globals[vm->global_len] = token_to_value(n->token, alloc);
      BC(OP_LOADG, vm->global_len++);
      break;
    }

    size_t bucket = hash & GLOBAL_MASK;
    size_t cached_index = ctx->global_hash_buckets[bucket];
    size_t expected_index = vm->global_len;

    if (cached_index) {
      expected_index = cached_index - 1;
    } else {
      ASSERT(vm->global_len + 1 < GLOBAL_SIZE,
             "cc: out of global space, what the fuck are you doing");
      ctx->global_hash_buckets[bucket] = vm->global_len + 1;
      vm->globals[vm->global_len++] = token_to_value(n->token, alloc);
    }

    BC(OP_LOADG, expected_index)
    break;
  }
  case N_IDENT: {
    uint64_t hash = n->token->string.hash & VARIABLE_TABLE_SIZE_MASK;
    BC(OP_LOADV, hash);
    break;
  }
  case N_BIN: {
    // single argument is just a return of that value
    if (n->children_length == 1) {
      // TODO: arithmetic optimisations like n+0=n; n*0=0; n*1=n, etc
      compile(alloc, vm, ctx, n->children[0]);
    } else if (n->children_length == 2) {
      // two arguments is easy to compile, just load and add two Values
      compile(alloc, vm, ctx, n->children[0]);
      size_t r = Ctx_allocate_register(ctx);
      BC(OP_STORE, r)
      compile(alloc, vm, ctx, n->children[1]);
      BC(n->token->type, r)
      Ctx_free_register(ctx, r);
    } else {
      TODO("compile#N_LIST for Node.children_length > 3 is not implemented");
    }
    break;
  }
  case N_BUILTIN: {
    Str *s = &n->token->string;
    int b = runtime_builtin_hashes[s->hash & MAX_BUILTIN_SIZE_MASK];
    // compile time "pseudo" builtins
    if (b != 0) {
      switch (b) {
      case COMPILE_BUILTIN_ASSERT: { // (@assert <s-expr>)
        for (size_t i = 0; i < n->children_length; i++) {
          compile(alloc, vm, ctx, n->children[i]);
        }
        BC(OP_ASSERT, 0);
        break;
      };
      case COMPILE_BUILTIN_NONE: { // (@None)
        BC(OP_LOADG, GLOBAL_NONE);
        break;
      }
      case COMPILE_BUILTIN_MATCH: { // (@match
                                    //  (<condition> <s-expr's>) ; case 1
                                    //  (<condition> <s-expr's>) ; case 2
                                    //  (<s-expr's>) ; default case
                                    //  )
        ASSERT(n->children_length >= 1,
               "Need at least one case for a match expr")

        // used for jumping to the end of the match statement once a case is
        // done
        int backfill_slots[n->children_length];
        bool encountered_default_cause = false;

        // iterating over cases
        for (size_t i = 0; i < n->children_length; i++) {
          Node *cur_case = n->children[i];

          if (cur_case->children_length > 1) {
            Node *cur_condition = cur_case->children[0];
            Node *cur_body = cur_case->children[1];
            compile(alloc, vm, ctx, cur_condition);
            size_t last_case_start = vm->bytecode_len;
            // jump to next case conditional if conditional above is false
            BC(OP_JMPF, 0xAFFEDEAD)

            compile(alloc, vm, ctx, cur_body);

            // only jmp to end of match statement if not already at the end,
            // since we are inside of the body of a case
            if (i != n->children_length - 1) {
              backfill_slots[i] = vm->bytecode_len;
              BC(OP_JMP, 0xAFFEDEAD)
            }
            vm->bytecode[last_case_start + 1] = vm->bytecode_len;
          } else {
            ASSERT(!encountered_default_cause,
                   "Only a single default case allowed")
            // default case has a singular children, being executed if all other
            // cases do not match
            compile(alloc, vm, ctx, cur_case);
            encountered_default_cause = true;
          }
        }

        // backfill jumps to the end of the switch statement
        for (size_t i = 0; i < n->children_length - 1; i++) {
          int jump_argument_location = backfill_slots[i];
          if (jump_argument_location) {
            vm->bytecode[jump_argument_location + 1] = vm->bytecode_len;
          }
        }

        break;
      }
      case COMPILE_BUILTIN_FUNCTION: { // (@function <name> [<args>] <s-expr's>)
        ASSERT(n->children_length >= 2,
               "@function expects <name> [<arguments>] [body]");
        Str *name = &n->children[0]->token->string;
        size_t hash = name->hash & MAX_BUILTIN_SIZE_MASK;
        Node **params = n->children[1]->children;
        size_t param_len = n->children[1]->children_length;

        CtxFunction function_ctx = {
            .name = &n->children[0]->token->string,
            .bytecode_index = vm->bytecode_len,
            .argument_count = param_len,
        };
        ctx->hash_to_function[hash] = function_ctx;

        // PERF: optimisation for removing empty functions
        if (n->children_length == 2) {
          DEBUG_PUTS("Removing body of empty `%.*s` function",
                     (int)function_ctx.name->len, function_ctx.name->p);
          return;
        }

        // this is the worst hack i have ever written, this is used to
        // jump over the bytecode of a function (header with args setup
        // and body), so we keep the bytecode compilation single pass and
        // the bytecode linear, this works (for now at least)
        size_t jump_op_index = vm->bytecode_len;
        BC(OP_JMP,
           0xAFFEDEAD); // https://de.wiktionary.org/wiki/Klappe_zu,_Affe_tot

        // Calling convention:
        //
        // registers  = r1 r2 r3
        // parameters = [a b c]
        // arguments  = [1 2 3]
        for (size_t i = 0; i < param_len; i++) {
          Node *param = params[i];
          ASSERT(param->type == N_IDENT,
                 "Expected identifier as function parameter in `%.*s` "
                 "definition, got `%.*s`",
                 (int)name->len, name->p, (int)NODE_TYPE_MAP[param->type].len,
                 NODE_TYPE_MAP[param->type].p);
          // PERF: changing args to start from r1 to starting from r0,
          // thus saving a single OP_LOAD for each function invocation
          BC(OP_LOAD, i + 1);
          BC(OP_VAR, param->token->string.hash & VARIABLE_TABLE_SIZE_MASK);
        }

        // compiling the body, returning a value is free since its just in
        // r0
        if (n->children_length > 2) {
          for (size_t i = 2; i < n->children_length; i++) {
            // PERF: if last Node is N_CALL think about reusing call
            // frames (TCO)
            compile(alloc, vm, ctx, n->children[i]);
          }
        }

        BC(OP_LEAVE, 0);
        vm->bytecode[jump_op_index + 1] = vm->bytecode_len;
        ctx->hash_to_function[hash].size =
            vm->bytecode_len - function_ctx.bytecode_index;
        break;
      }
      case COMPILE_BUILTIN_LET: { // (@len <var-name> <var-value>)
        ASSERT(n->children_length == 2,
               "@let requires two arguments: `@let "
               "<var-name> <var-value>`, got %zu",
               n->children_length);
        compile(alloc, vm, ctx, n->children[1]);
        Token *ident = n->children[0]->token;
        size_t hash = ident->string.hash & VARIABLE_TABLE_SIZE_MASK;
        BC(OP_VAR, hash);
        break;
      }
      default:
      }
    } else { // calling a builtin function thats not a compile time construct
      size_t hash = s->hash & MAX_BUILTIN_SIZE_MASK;
      builtin_function bf = vm->builtins[hash];
      ASSERT(bf != NULL, "Unknown builtin `@%.*s`", (int)s->len, s->p)

      size_t len = n->children_length == 0 ? 1 : n->children_length;
      size_t registers[len];
      for (size_t i = 0; i < n->children_length; i++) {
        compile(alloc, vm, ctx, n->children[i]);
        size_t r = Ctx_allocate_register(ctx);
        registers[i] = r;
        BC(OP_STORE, r)
      }
      for (int i = n->children_length - 1; i >= 0; i--) {
        Ctx_free_register(ctx, registers[i]);
      }

      if (n->children_length > 1) {
        BC(OP_ARGS, n->children_length);
      }

      BC(OP_BUILTIN, hash);
    }
    break;
  }
  case N_CALL: { // function call site (<name> <args>)
    Str *name = &n->token->string;
    CtxFunction *func =
        &ctx->hash_to_function[name->hash & MAX_BUILTIN_SIZE_MASK];
    ASSERT(func->name != NULL, "Undefined function `%.*s`", (int)name->len,
           name->p)
    ASSERT(n->children_length == func->argument_count,
           "`%.*s` wants %zu arguments, got %zu", (int)func->name->len,
           func->name->p, func->argument_count, n->children_length);

    // PERF: optimisation to remove calls to empty functions, since their
    // definition is also removed
    if (!func->size) {
      DEBUG_PUTS("Removing call to empty `%.*s` function (size=%zu)",
                 (int)func->name->len, func->name->p, func->size);
      return;
    }

    // we compile all arguments to bytecode one by one by one
    size_t registers[n->children_length];
    for (size_t i = 0; i < n->children_length; i++) {
      compile(alloc, vm, ctx, n->children[i]);
      size_t r = Ctx_allocate_register(ctx);
      registers[i] = r;
      BC(OP_STORE, r)
    }
    for (int i = n->children_length - 1; i >= 0; i--) {
      Ctx_free_register(ctx, registers[i]);
    }
    if (n->children_length > 1) {
      BC(OP_ARGS, n->children_length);
    }

    BC(OP_CALL, func->bytecode_index);
    break;
  }
  case N_OBJECT: {
    ASSERT(vm->global_len + 1 < GLOBAL_SIZE,
           "cc: out of global space, what the fuck are you doing");
    Value *v = CALL(alloc, request, sizeof(Value));
    v->type = V_OBJ;
    // TODO: correct init here
    v->obj = (Map){.size = 0};
    vm->globals[vm->global_len] = v;
    BC(OP_LOADG, vm->global_len++);
    break;
  }
  case N_LIST: // Arrays are just sugar for lists, same same
  case N_ARRAY: {
    if (n->children_length == 0) {
      ASSERT(vm->global_len + 1 < GLOBAL_SIZE,
             "cc: out of global space, what the fuck are you doing");
      Value *v = CALL(alloc, request, sizeof(Value));
      v->type = V_ARRAY;
      // TODO: correct init here
      v->array = (List){.len = 0, .cap = 0};
      vm->globals[vm->global_len] = v;
      BC(OP_LOADG, vm->global_len++)
    } else {
      TODO("N_ARRAY#statically allocate known array sizes")
    }
    break;
  }

  default:
    Str *s = &NODE_TYPE_MAP[n->type];
    ASSERT(0,
           "Compiling NODE[%.*s] is not implemented yet, sorry, you can "
           "contribute at https://github.com/xNaCly/purple-garden",
           (int)s->len, s->p);
    break;
  }
}

#define NEW_CC_BUILTIN(NAME, ENUM_VARIANT)                                     \
  runtime_builtin_hashes[Str_hash(&STRING(NAME)) & MAX_BUILTIN_SIZE_MASK] =    \
      COMPILE_BUILTIN_##ENUM_VARIANT;

Ctx cc(Vm *vm, Allocator *alloc, Node **nodes, size_t size) {
  // compile time constructs
  NEW_CC_BUILTIN("let", LET)
  NEW_CC_BUILTIN("function", FUNCTION)
  NEW_CC_BUILTIN("assert", ASSERT)
  NEW_CC_BUILTIN("None", NONE)
  NEW_CC_BUILTIN("match", MATCH)

  // specifically set size 1 to keep r0 the temporary register reserved
  Ctx ctx = {
      .register_allocated_count = 1,
      .registers = {0},
      .global_hash_buckets = CALL(alloc, request, sizeof(size_t) * GLOBAL_SIZE),
      .hash_to_function = {},
  };

  for (size_t i = 0; i < size; i++) {
    compile(alloc, vm, &ctx, nodes[i]);
  }
  return ctx;
}

#undef BC
#undef TODO

Ctx cc_seeded(Vm *vm, Allocator *alloc, Node **nodes, size_t size,
              const Ctx *seed) {
  // compile time constructs
  NEW_CC_BUILTIN("let", LET)
  NEW_CC_BUILTIN("function", FUNCTION)
  NEW_CC_BUILTIN("assert", ASSERT)
  NEW_CC_BUILTIN("None", NONE)
  NEW_CC_BUILTIN("match", MATCH)

  Ctx ctx = {
      .register_allocated_count = 1,
      .registers = {0},
      .global_hash_buckets = CALL(alloc, request, sizeof(size_t) * GLOBAL_SIZE),
      .hash_to_function = {},
  };

  if (seed != NULL) {
    // carry over known functions
    memcpy(ctx.hash_to_function, seed->hash_to_function,
           sizeof(ctx.hash_to_function));
    // carry over global interning buckets to avoid duplicates
    if (seed->global_hash_buckets) {
      memcpy(ctx.global_hash_buckets, seed->global_hash_buckets,
             sizeof(size_t) * GLOBAL_SIZE);
    }
  }

  for (size_t i = 0; i < size; i++) {
    compile(alloc, vm, &ctx, nodes[i]);
  }
  return ctx;
}

