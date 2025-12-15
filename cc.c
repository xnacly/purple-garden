#include <stdlib.h>

#include "adts.h"
#include "cc.h"
#include "common.h"
#include "lexer.h"
#include "mem.h"
#include "parser.h"
#include "std/std.h"
#include "strings.h"
#include "vm.h"

#if DEBUG
#define DEBUG_PUTS(fmt, ...)                                                   \
  do {                                                                         \
    printf("[cc] " fmt "\n", ##__VA_ARGS__);                                   \
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
    v.type = V_STR;
    v.string = t->string;
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
  printf("reserving r%zu\n", ctx->register_allocated_count);
#endif
  return ctx->register_allocated_count++;
}

static void Ctx_free_register(Ctx *ctx, size_t i) {
  ASSERT(i < ctx->register_allocated_count, "cc: register index out of bounds");
  ASSERT(ctx->registers[i], "cc: attempting to free unreserved register");
  ctx->register_allocated_count--;
#if DEBUG
  printf("freeing r%zu\n", ctx->register_allocated_count);
#endif
  ctx->registers[i] = false;
}

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
    BC(OP_LOADV, (uint32_t)n->token->string.hash);
    break;
  }
  case N_BIN: {
    // single argument is just a return of that value
    if (n->children.len == 1) {
      Node *child = LIST_get_UNSAFE(&n->children, 0);
      // PERF: arithmetic optimisations like n+0=n; n*0=0; n*1=n, etc
      compile(alloc, vm, ctx, child);
    } else {
      compile(alloc, vm, ctx, LIST_get_UNSAFE(&n->children, 0));
      size_t r = Ctx_allocate_register(ctx);
      BC(OP_STORE, r);
      compile(alloc, vm, ctx, LIST_get_UNSAFE(&n->children, 1));
      BC(n->token->type, r);
      Ctx_free_register(ctx, r);
    }
    break;
  }
  case N_VAR: {
    Node *value = LIST_get_UNSAFE(&n->children, 0);
    compile(alloc, vm, ctx, value);
    BC(OP_VAR, (uint32_t)n->token->string.hash);
    break;
  }
  case N_FOR: {

    // for <ident> :: <target> { <body> }

    // hash of <ident>
    uint64_t variable_hash = n->token->string.hash;

    Node *target_node = LIST_get(&n->children, 0);
    compile(alloc, vm, ctx, target_node);
    size_t r_target = Ctx_allocate_register(ctx);
    BC(OP_STORE, r_target);

    // len(<target>)
    BC(OP_LEN, r_target)
    size_t r_len = Ctx_allocate_register(ctx);
    BC(OP_STORE, r_len);

    // we want to bail early, if len(<target>) < 0
    BC(OP_LOADI, 0);
    BC(OP_LT, r_len);
    size_t backfill_for_skipping_on_zero_len = BC_LEN;
    BC(OP_JMPF, 0xAFFEDEAD);

    // this keeps track of the current iteration
    size_t r_i = Ctx_allocate_register(ctx);

    // i = 0
    BC(OP_LOADI, 0);
    BC(OP_STORE, r_i);
    size_t start_label = BC_LEN;

    // <ident> = <target>[<i>]
    BC(OP_LOAD, r_i);

    BC(OP_LT, r_len);
    size_t backfill_for_ending_loop_iter = BC_LEN;
    BC(OP_JMPF, 0xAFFEDEAD)
    BC(OP_LOAD, r_i);

    // extract member of <target> at <iterator>
    BC(OP_IDX, r_target);
    // put first member into the variable table at variable_hash
    BC(OP_VAR, variable_hash);

    // compile <body>
    Node *body = LIST_get(&n->children, 1);
    for (size_t body_i = 0; body_i < body->children.len; body_i++) {
      compile(alloc, vm, ctx, LIST_get(&body->children, body_i));
    }

    // post loop increment (i++ / r_i = r_i + 1)
    BC(OP_LOADI, 1);
    BC(OP_ADD, r_i);
    BC(OP_STORE, r_i);
    BC(OP_JMP, start_label);

    ByteCodeBuilder_insert_arg(ctx->bcb, backfill_for_skipping_on_zero_len,
                               BC_LEN);
    ByteCodeBuilder_insert_arg(ctx->bcb, backfill_for_ending_loop_iter, BC_LEN);

    Ctx_free_register(ctx, r_i);
    Ctx_free_register(ctx, r_len);
    Ctx_free_register(ctx, r_target);
    break;
  }
  case N_PATH: {
    // stdlib path lookup and compilation
    if (n->token->type == T_STD) {
      size_t len = n->children.len;

      StdNode *sn = ctx->std;

      // walk path fragments to find package, for instance for
      // std::runtime::gc::stats() we walk: PKG: runtime PKG: gc FN: stats
      for (size_t i = 0; i < len; i++) {
        Str s = LIST_get_UNSAFE(&n->children, i)->token->string;
        for (size_t j = 0; j < sn->len; j++) {
          if (sn->children[j].name.hash == s.hash) {
            sn = &sn->children[j];
            break;
          }
        }
      }

      ASSERT(sn->fn != NULL, "No matching builtin function found");
      DEBUG_PUTS("Found std leaf function `%.*s`: %p", (int)sn->name.len,
                 sn->name.p, sn->fn);

      Node *call = LIST_get_UNSAFE(&n->children, len - 1);
      ASSERT(call != NULL, "Wasnt able to get the call part of the path");
      ASSERT(call->type == N_CALL, "Last segment of std access isn't N_CALL");
      size_t argument_len = call->children.len;

      size_t registers[argument_len < 1 ? 1 : argument_len];

      // -1 is the sentinel for a variable amount of arguments
      if (sn->argument_count > 0) {
        ASSERT((size_t)sn->argument_count == argument_len,
               "Argument count doesn't match");
      }

      for (size_t i = 0; i < argument_len; i++) {
        Node *child = LIST_get_UNSAFE(&call->children, i);
        compile(alloc, vm, ctx, child);
        size_t r = Ctx_allocate_register(ctx);
        registers[i] = r;
        BC(OP_STORE, r);
      }

      for (int i = argument_len - 1; i >= 0; i--) {
        Ctx_free_register(ctx, registers[i]);
      }

      BC(OP_ARGS, ENCODE_ARG_COUNT_AND_OFFSET(argument_len,
                                              ctx->register_allocated_count));
      BC(OP_SYS, vm->builtin_count++);
      LIST_append(&vm->builtins, vm->staticalloc, sn->fn);
    } else {
      compile(alloc, vm, ctx, LIST_get_UNSAFE(&n->children, 0));
      size_t rtarget = Ctx_allocate_register(ctx);
      BC(OP_STORE, rtarget);
      for (size_t i = 1; i < n->children.len; i++) {
        compile(alloc, vm, ctx, LIST_get_UNSAFE(&n->children, i));
        BC(OP_IDX, rtarget);
      }

      Ctx_free_register(ctx, rtarget);
    }

    break;
  }
  case N_MATCH: {
    size_t skip_backfill_slots[n->children.len];

    for (size_t i = 0; i < n->children.len; i++) {
      Node *match_case = LIST_get_UNSAFE(&n->children, i);
      if (match_case->type == N_CASE) {
        // compile condition
        compile(alloc, vm, ctx, LIST_get_UNSAFE(&match_case->children, 0));
        size_t next_case = BC_LEN;
        BC(OP_JMPF, 0xAFFEDEAD);

        // compile the expressions making up the body
        for (size_t j = 1; j < match_case->children.len; j++) {
          compile(alloc, vm, ctx, LIST_get_UNSAFE(&match_case->children, j));
        }

        skip_backfill_slots[i] = BC_LEN;
        // skip to the end of the match block
        BC(OP_JMP, 0xAFFEDEAD);

        // only jump to the next case if there is one, which there only can be
        // if we are not at the last case
        if (i + 1 < n->children.len) {
          ByteCodeBuilder_insert_arg(ctx->bcb, next_case, BC_LEN);
        }
      } else {
        skip_backfill_slots[i] = 0;
        for (size_t j = 0; j < match_case->children.len; j++) {
          compile(alloc, vm, ctx, LIST_get_UNSAFE(&match_case->children, j));
        }
      }
    }

    size_t size = BC_LEN;
    for (size_t i = 0; i < n->children.len; i++) {
      size_t slot = skip_backfill_slots[i];
      if (slot) {
        ByteCodeBuilder_insert_arg(ctx->bcb, slot, size);
      }
    }

    break;
  };
  case N_FN: {
    Str name = n->token->string;
    size_t hash = name.hash & MAX_BUILTIN_SIZE_MASK;
    LIST_Nptr params = LIST_get_UNSAFE(&n->children, 0)->children;

    // ASSERT(ctx->hash_to_function[hash].name.len == 0,
    //        "Cant redefine function `%.*s`", (int)name.len, name.p);

    CtxFunction function_ctx = {
        .name = name,
        .bytecode_index = BC_LEN,
        .argument_count = params.len,
    };
    ctx->hash_to_function[hash] = function_ctx;

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
    for (size_t i = 0; i < params.len; i++) {
      Node *param = LIST_get_UNSAFE(&params, i);
      BC(OP_LOAD, i + 1);
      BC(OP_VAR, param->token->string.hash);
    }

    // compiling the body, returning a value is free since its just in
    // r0
    if (n->children.len > 1) {
      for (size_t i = 1; i < n->children.len; i++) {
        // PERF: if last Node is N_CALL think about reusing call
        // frames (TCO)
        Node *body_expr = LIST_get_UNSAFE(&n->children, i);
        compile(alloc, vm, ctx, body_expr);
      }
    }

    BC(OP_RET, 0);
    ByteCodeBuilder_insert_arg(ctx->bcb, jump_op_index, BC_LEN);
    ctx->hash_to_function[hash].size = BC_LEN - function_ctx.bytecode_index;

    DEBUG_PUTS("Compiled fn `%.*s` {.bytecode_index=%zu, .argument_count=%zu, "
               ".size=%lu}",
               (int)function_ctx.name.len, function_ctx.name.p,
               function_ctx.bytecode_index, function_ctx.argument_count,
               BC_LEN - function_ctx.bytecode_index);

    break;
  }
  case N_CALL: { // user defined function call site (<name> <args>)
    Str *name = &n->token->string;
    size_t len = n->children.len;

    CtxFunction *func = func =
        &ctx->hash_to_function[name->hash & MAX_BUILTIN_SIZE_MASK];
    ASSERT(func->name.len != 0, "Undefined function `%.*s`", (int)name->len,
           name->p)
    ASSERT(len == func->argument_count, "`%.*s` wants %zu arguments, got %zu",
           (int)func->name.len, func->name.p, func->argument_count, len);

    // we compile all arguments to bytecode one by one by one
    size_t registers[len < 1 ? 1 : len];
    for (size_t i = 0; i < len; i++) {
      Node *child = LIST_get_UNSAFE(&n->children, i);
      compile(alloc, vm, ctx, child);
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
    size_t size = n->children.len;
    // fast path for empty obj
    if (size != 0) {
      // size hint is placed in r0 to instruct the OP_NEW to use the
      // allocation size for any value, such as an array or object.
      BC(OP_SIZE, size);
    }

    BC(OP_NEW, VM_NEW_OBJ);

    // fast path for empty obj
    if (size != 0) {
      // after OP_NEW the created value is in r0, we must now temporarly
      // move it to any other register, so its not clobbered by acm
      // register usage
      size_t obj_register = Ctx_allocate_register(ctx);
      BC(OP_STORE, obj_register);
      // TODO("There is no pg instruction for inserting into an object yet")

      for (size_t i = 0; i < size; i++) {
        Node *member = LIST_get_UNSAFE(&n->children, i);
        compile(alloc, vm, ctx, member);
      }

      // move the array back into r0, since it needs to be the return
      // value of this N_ARRAY and N_LIST node
      BC(OP_LOAD, obj_register);
      Ctx_free_register(ctx, obj_register);
    }
    break;
  }
  case N_ARRAY: {
    size_t size = n->children.len;
    // fast path for empty array
    if (size != 0) {
      // size hint is placed in r0 to instruct the OP_NEW to use the
      // allocation size for any value, such as an array or object.
      BC(OP_SIZE, size);
    }

    BC(OP_NEW, VM_NEW_ARRAY);

    // fast path for empty array
    if (size != 0) {
      // after OP_NEW the created value is in r0, we must now temporarly
      // move it to any other register, so its not clobbered by acm
      // register usage
      size_t list_register = Ctx_allocate_register(ctx);
      BC(OP_STORE, list_register);

      for (size_t i = 0; i < size; i++) {
        Node *member = LIST_get_UNSAFE(&n->children, i);
        compile(alloc, vm, ctx, member);
        BC(OP_APPEND, list_register);
      }

      // move the array back into r0, since it needs to be the return
      // value of this N_ARRAY and N_LIST node
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

Ctx cc(Vm *vm, Allocator *alloc, Parser *p) {
  ByteCodeBuilder bcb = ByteCodeBuilder_new(alloc);

  // specifically set size 1 to keep r0 the temporary register reserved
  Ctx ctx = {
      .register_allocated_count = 1,
      .registers = {0},
      .global_hash_buckets = {0},
      .hash_to_function = {},
      .bcb = &bcb,
      .std = std_tree(vm->config, alloc),
  };

  while (true) {
    Node *n = Parser_next(p);
    if (!n) {
      break;
    }

#if DEBUG
    Node_debug(n, 0);
    puts("");
#endif
    compile(alloc, vm, &ctx, n);
  }

  ASSERT(ctx.register_allocated_count == 1,
         "Not all registers were freed, compiler bug!");

  vm->bytecode = ByteCodeBuilder_to_buffer(ctx.bcb);
  vm->bytecode_len = ctx.bcb->buffer.len;
  return ctx;
}

#undef BC
#undef TODO
