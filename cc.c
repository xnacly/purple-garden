#include <stdlib.h>

#include "adts.h"
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
inline static Value token_to_value(Token *t, Allocator *a) {
  Value v;
  switch (t->type) {
  case T_STRING:
  case T_IDENT:
    Str *s = CALL(a, request, sizeof(Str));
    *s = t->string;
    v.type = V_STR;
    v.string = s;
    break;
  case T_TRUE:
    v.type = V_TRUE;
    break;
  case T_FALSE:
    v.type = V_FALSE;
    break;
  case T_INTEGER:
    v.type = V_INT;
    v.integer = Str_to_int64_t(&t->string);
    break;
  case T_DOUBLE:
    v.type = V_DOUBLE;
    v.floating = Str_to_double(&t->string);
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
  ASSERT(i < ctx->register_allocated_count, "cc: register index out of bounds");
  ASSERT(ctx->registers[i], "cc: attempting to free unallocated register");
  ctx->register_allocated_count--;
#if DEBUG
  printf("freeing r%zu\n", ctx->register_allocated_count);
#endif
  ctx->registers[i] = false;
}

static size_t runtime_builtin_hashes[MAX_BUILTIN_SIZE + 1];

static void compile(Allocator *alloc, Vm *vm, Ctx *ctx, const Node *n) {
  switch (n->type) {
  case N_ATOM: { // TODO: this works but not good enough and its somewhat silly
                 // to think we wouldnt need a collision strategy for buckets
// High tag bits
#define TAG_STRING 0x3000000000000000ULL
#define TAG_DOUBLE 0x4000000000000000ULL
#define TAG_INT 0x5000000000000000ULL

// Tag mask to keep lower bits
#define TAG_MASK 0x0FFFFFFFFFFFFFFFULL

    size_t hash = 0;
    if (n->token->type == T_FALSE) {
      BC(OP_LOADG, GLOBAL_FALSE);
      break;
    } else if (n->token->type == T_TRUE) {
      BC(OP_LOADG, GLOBAL_TRUE);
      break;
    } else if (n->token->type == T_STRING) {
      hash = TAG_STRING | (n->token->string.hash & TAG_MASK);
    } else if (n->token->type == T_DOUBLE) {
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

    size_t bucket = hash & GLOBAL_SIZE_MASK;
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

    BC(OP_LOADG, expected_index);
    break;
  }
  case N_IDENT: {
    uint64_t hash = n->token->string.hash & VARIABLE_TABLE_SIZE_MASK;
    BC(OP_LOADV, hash);
    break;
  }
  case N_BIN: {
    // single argument is just a return of that value
    if (n->children.len == 1) {
      Node child = LIST_get(&n->children, 0);
      // PERF: arithmetic optimisations like n+0=n; n*0=0; n*1=n, etc
      compile(alloc, vm, ctx, &child);
    } else if (n->children.len == 2) {
      Node lhs = LIST_get(&n->children, 0);
      // two arguments is easy to compile, just load and add two Values
      compile(alloc, vm, ctx, &lhs);
      size_t r = Ctx_allocate_register(ctx);
      BC(OP_STORE, r);
      Node rhs = LIST_get(&n->children, 1);
      compile(alloc, vm, ctx, &rhs);
      BC(n->token->type, r);
      Ctx_free_register(ctx, r);
    } else {
#if DEBUG
      Node_debug(n, 0);
#endif
      TODO("compile#N_LIST for Node.children_length > 3 is not implemented");
    }
    break;
  }
  case N_BUILTIN: {
    Str s = n->token->string;
    int b = runtime_builtin_hashes[s.hash & MAX_BUILTIN_SIZE_MASK];
    // compile time "pseudo" builtins
    if (b != 0) {
      switch (b) {
      case COMPILE_BUILTIN_ASSERT: { // (@assert <s-expr>)
        for (size_t i = 0; i < n->children.len; i++) {
          Node child = LIST_get(&n->children, i);
          compile(alloc, vm, ctx, &child);
        }
        BC(OP_ASSERT, 0);
        break;
      };
      case COMPILE_BUILTIN_NONE: { // (@None)
        BC(OP_LOADG, GLOBAL_NONE);
        break;
      }
        // TODO: unsupported
      case COMPILE_BUILTIN_MATCH: { // (@match
                                    //  (<condition> <s-expr's>) ; case 1
                                    //  (<condition> <s-expr's>) ; case 2
                                    //  Node ; default case
                                    //  )

        size_t len = n->children.len;
        ASSERT(len >= 1, "Need at least one case for a match expr")

        // used for jumping to the end of the match statement once a case is
        // done
        int backfill_slots[len];
        bool encountered_default_cause = false;

        // iterating over cases
        for (size_t i = 0; i < len; i++) {
          Node cur_case = LIST_get(&n->children, i);

          if (cur_case.children.len > 1) {
            Node cur_condition = LIST_get(&cur_case.children, 0);
            Node cur_body = LIST_get(&cur_case.children, 1);
            compile(alloc, vm, ctx, &cur_condition);
            size_t last_case_start = BC_LEN;
            // jump to next case conditional if conditional above is false
            BC(OP_JMPF, 0xAFFEDEAD);

            compile(alloc, vm, ctx, &cur_body);

            // only jmp to end of match statement if not already at the end,
            // since we are inside of the body of a case
            if (i != len - 1) {
              backfill_slots[i] = BC_LEN;
              BC(OP_JMP, 0xAFFEDEAD);
            }
            ByteCodeBuilder_insert_arg(ctx->bcb, last_case_start, BC_LEN);
          } else {
            ASSERT(!encountered_default_cause,
                   "Only a single default case allowed")
            // default case has a singular children, being executed if all other
            // cases do not match
            compile(alloc, vm, ctx, &cur_case);
            encountered_default_cause = true;
          }
        }

        // backfill jumps to the end of the switch statement
        for (size_t i = 0; i < len - 1; i++) {
          int jump_argument_location = backfill_slots[i];
          if (jump_argument_location) {
            ByteCodeBuilder_insert_arg(ctx->bcb, jump_argument_location,
                                       BC_LEN);
          }
        }

        break;
      }
      case COMPILE_BUILTIN_FUNCTION: { // (@function <name> [<args>] <s-expr's>)
        size_t len = n->children.len;
        ASSERT(len >= 2, "@function expects <name> [<arguments>] [body]");
        Str name = LIST_get(&n->children, 0).token->string;
        size_t hash = name.hash & MAX_BUILTIN_SIZE_MASK;
        LIST_Node params = LIST_get(&n->children, 1).children;
        size_t param_len = params.len;

        CtxFunction function_ctx = {
            .name = name,
            .bytecode_index = BC_LEN,
            .argument_count = param_len,
        };
        ctx->hash_to_function[hash] = function_ctx;

        // PERF: optimisation for removing empty functions
        if (len == 2) {
          DEBUG_PUTS("Removing body of empty `%.*s` function",
                     (int)function_ctx.name.len, function_ctx.name.p);
          return;
        }

        // this is the worst hack i have ever written, this is used to
        // jump over the bytecode of a function (header with args setup
        // and body), so we keep the bytecode compilation single pass and
        // the bytecode linear, this works (for now at least)
        size_t jump_op_index = BC_LEN;
        BC(OP_JMP,
           0xAFFEDEAD); // https://de.wiktionary.org/wiki/Klappe_zu,_Affe_tot

        // Calling convention:
        //
        // registers  = rn1 rn2 rn3
        // parameters = [ a  b  c]
        // arguments  = [ 1  2  3]
        for (size_t i = 0; i < param_len; i++) {
          Node param = LIST_get(&params, i);
          ASSERT(param.type == N_IDENT,
                 "Expected identifier as function parameter in `%.*s` "
                 "definition, got `%.*s`",
                 (int)name.len, name.p, (int)NODE_TYPE_MAP[param.type].len,
                 NODE_TYPE_MAP[param.type].p);

          // PERF: changing args to start from r1 to starting from r0,
          // thus saving a single OP_LOAD for each function invocation
          BC(OP_LOAD, i + 1);
          BC(OP_VAR, param.token->string.hash & VARIABLE_TABLE_SIZE_MASK);
        }

        // compiling the body, returning a value is free since its just in
        // r0
        if (len > 2) {
          for (size_t i = 2; i < len; i++) {
            // PERF: if last Node is N_CALL think about reusing call
            // frames (TCO)
            Node body_expr = LIST_get(&n->children, i);
            compile(alloc, vm, ctx, &body_expr);
          }
        }

        BC(OP_LEAVE, 0);
        ByteCodeBuilder_insert_arg(ctx->bcb, jump_op_index, BC_LEN);
        ctx->hash_to_function[hash].size = BC_LEN - function_ctx.bytecode_index;
        break;
      }
      case COMPILE_BUILTIN_LET: { // (@len <var-name> <var-value>)
        size_t len = n->children.len;
        ASSERT(len == 2,
               "@let requires two arguments: `@let "
               "<var-name> <var-value>`, got %zu",
               len);
        Node rhs = LIST_get(&n->children, 1);
        compile(alloc, vm, ctx, &rhs);
        Token *ident = LIST_get(&n->children, 0).token;
        size_t hash = ident->string.hash & VARIABLE_TABLE_SIZE_MASK;
        BC(OP_VAR, hash);
        break;
      }
      default:
      }
    } else { // calling a builtin function thats not a compile time construct
      size_t len = n->children.len;
      // TODO: once namespaces drop (only the compile time ones) walk the
      // namespaces and replace bf with the idx into builtin_function which
      // contains the resolved pointer to said function
      size_t idx = s.hash & MAX_BUILTIN_SIZE_MASK;
      builtin_function bf = vm->builtins[idx];
      ASSERT(bf != NULL, "Unknown builtin `@%.*s`", (int)s.len, s.p)

      if (len > 0) {
        size_t registers[len];
        for (size_t i = 0; i < len; i++) {
          Node argument = LIST_get(&n->children, i);
          compile(alloc, vm, ctx, &argument);
          size_t r = Ctx_allocate_register(ctx);
          registers[i] = r;
          BC(OP_STORE, r);
        }
        for (int i = len - 1; i >= 0; i--) {
          Ctx_free_register(ctx, registers[i]);
        }
      }

      BC(OP_ARGS,
         ENCODE_ARG_COUNT_AND_OFFSET(len, ctx->register_allocated_count));
      BC(OP_BUILTIN, idx);
    }
    break;
  }
  case N_CALL: { // function call site (<name> <args>)
    Str *name = &n->token->string;
    size_t len = n->children.len;
    CtxFunction *func =
        &ctx->hash_to_function[name->hash & MAX_BUILTIN_SIZE_MASK];
    ASSERT(func->name.len != 0, "Undefined function `%.*s`", (int)name->len,
           name->p)
    ASSERT(len == func->argument_count, "`%.*s` wants %zu arguments, got %zu",
           (int)func->name.len, func->name.p, func->argument_count, len);

    // PERF: optimisation to remove calls to empty functions, since their
    // definition is also removed
    if (!func->size) {
      DEBUG_PUTS("Removing call to empty `%.*s` function (size=%zu)",
                 (int)func->name.len, func->name.p, func->size);
      return;
    }

    size_t children_length = len < 1 ? 1 : len;
    // we compile all arguments to bytecode one by one by one
    size_t registers[children_length];
    for (size_t i = 0; i < len; i++) {
      Node child = LIST_get(&n->children, i);
      compile(alloc, vm, ctx, &child);
      size_t r = Ctx_allocate_register(ctx);
      registers[i] = r;
      BC(OP_STORE, r);
    }

    for (int i = len - 1; i >= 0; i--) {
      Ctx_free_register(ctx, registers[i]);
    }

    BC(OP_ARGS,
       ENCODE_ARG_COUNT_AND_OFFSET(len, ctx->register_allocated_count));
    BC(OP_CALL, func->bytecode_index);
    break;
  }
  case N_OBJECT: {
    // TODO: its time, see N_LIST|N_ARRAY
    ASSERT(0, "N_OBJECT: Unimplemented");
    break;
  }
  case N_LIST: // Lists are just sugar for arrays, same same
  case N_ARRAY: {
    size_t size = n->children.len;
    // fast path for empty array
    if (size != 0) {
      // size hint is placed in r0 to instruct the OP_NEW to use the allocation
      // size for any value, such as an array or object.
      BC(OP_SIZE, size);
    }

    BC(OP_NEW, VM_NEW_ARRAY);

    // fast path for empty array
    if (size != 0) {
      // after OP_NEW the created value is in r0, we must now temporarly move
      // it to any other register, so its not clobbered by acm register usage
      size_t list_register = Ctx_allocate_register(ctx);
      BC(OP_STORE, list_register);

      for (size_t i = 0; i < size; i++) {
        Node member = LIST_get(&n->children, i);
        compile(alloc, vm, ctx, &member);
        BC(OP_APPEND, list_register);
      }

      // move the array back into r0, since it needs to be the return value of
      // this N_ARRAY and N_LIST node
      BC(OP_LOAD, list_register);
      Ctx_free_register(ctx, list_register);
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

// Creates a compiler builtin
#define NEW_CC_BUILTIN(NAME, ENUM_VARIANT)                                     \
  runtime_builtin_hashes[Str_hash(&STRING(NAME)) & MAX_BUILTIN_SIZE_MASK] =    \
      COMPILE_BUILTIN_##ENUM_VARIANT;

Ctx cc(Vm *vm, Allocator *alloc, Parser *p) {
  // compile time constructs
  NEW_CC_BUILTIN("let", LET)
  NEW_CC_BUILTIN("fn", FUNCTION)
  NEW_CC_BUILTIN("assert", ASSERT)
  NEW_CC_BUILTIN("None", NONE)
  NEW_CC_BUILTIN("match", MATCH)

  ByteCodeBuilder bcb = ByteCodeBuilder_new(alloc);

  // specifically set size 1 to keep r0 the temporary register reserved
  Ctx ctx = {
      .register_allocated_count = 1,
      .registers = {0},
      .global_hash_buckets = {0},
      .hash_to_function = {},
      .bcb = &bcb,
  };

  while (true) {
    Node n = Parser_next(p);
    if (n.type == N_UNKNOWN) {
      break;
    }

#if DEBUG
    Node_debug(&n, 0);
    puts("");
#endif
    compile(alloc, vm, &ctx, &n);
  }

  ASSERT(ctx.register_allocated_count == 1,
         "Not all registers were freed, compiler bug!");

  vm->bytecode = ByteCodeBuilder_to_buffer(ctx.bcb);
  vm->bytecode_len = ctx.bcb->buffer.len;
  return ctx;
}

#undef BC
#undef TODO
