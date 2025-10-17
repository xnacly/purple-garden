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
      Node *child = LIST_get_UNSAFE(&n->children, 0);
      // PERF: arithmetic optimisations like n+0=n; n*0=0; n*1=n, etc
      compile(alloc, vm, ctx, child);
    } else {
      Node *lhs = LIST_get_UNSAFE(&n->children, 0);
      // two arguments is easy to compile, just load and add two Values
      compile(alloc, vm, ctx, lhs);
      size_t r = Ctx_allocate_register(ctx);
      BC(OP_STORE, r);
      Node *rhs = LIST_get_UNSAFE(&n->children, 1);
      compile(alloc, vm, ctx, rhs);
      BC(n->token->type, r);
      Ctx_free_register(ctx, r);
    }
    break;
  }
  case N_VAR: {
    size_t hash = n->token->string.hash & VARIABLE_TABLE_SIZE_MASK;
    Node *value = LIST_get_UNSAFE(&n->children, 0);
    compile(alloc, vm, ctx, value);
    BC(OP_VAR, hash);
    break;
  }
  case N_PATH: {

    // stdlib path lookup and compilation
    if (n->token->type == T_STD) {
      size_t len = n->children.len;

      StdNode *sn = ctx->std;

      // walk path segments to find package, for instance for
      // std::runtime::gc::stats() we walk: PKG: runtime PKG: gc FN: stats
      for (size_t i = 0; i < len; i++) {
        Str s = LIST_get_UNSAFE(&n->children, i)->token->string;
        for (size_t j = 0; j < sn->len; j++) {
          if (sn->children[j].name.hash == s.hash) {
            sn = &sn->children[j];
            DEBUG_PUTS("path segment `%.*s`", (int)s.len, s.p);
            break;
          }
        }
      }

      ASSERT(sn->fn != NULL, "No matching builtin function found");

      Node *last = LIST_get_UNSAFE(&n->children, len - 1);
      ASSERT(last != NULL, "Wasnt able to get the call part of the path");
      ASSERT(last->type == N_CALL, "Last segment of std access isn't N_CALL");
      size_t argument_len = last->children.len;

      size_t registers[argument_len < 1 ? 1 : len];
      for (size_t i = 0; i < argument_len; i++) {
        Node *child = LIST_get_UNSAFE(&last->children, i);
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
      BC(OP_BUILTIN, vm->builtin_count);
      Vm_register_builtin(vm, sn->fn);
    } else {
      ASSERT(0, "Only N_PATH for std currently supported")
    }

    break;
  }
  case N_FN: { // (fn <name> [<args>] <s-expr's>)
    Str name = n->token->string;
    size_t hash = name.hash & MAX_BUILTIN_SIZE_MASK;
    LIST_Nptr params = LIST_get_UNSAFE(&n->children, 0)->children;

    ASSERT(ctx->hash_to_function[hash].name.len == 0,
           "Cant redefine function `%.*s`", (int)name.len, name.p);

    CtxFunction function_ctx = {
        .name = name,
        .bytecode_index = BC_LEN,
        .argument_count = params.len,
        // placeholder so recursive self calls arent optimised away due to
        // function size checks
        .size = 0xAFFEDEAD,
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
      BC(OP_VAR, param->token->string.hash & VARIABLE_TABLE_SIZE_MASK);
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

    BC(OP_LEAVE, 0);
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
      // size hint is placed in r0 to instruct the OP_NEW to use the allocation
      // size for any value, such as an array or object.
      BC(OP_SIZE, size);
    }

    BC(OP_NEW, VM_NEW_OBJ);

    // fast path for empty obj
    if (size != 0) {
      // after OP_NEW the created value is in r0, we must now temporarly move
      // it to any other register, so its not clobbered by acm register usage
      size_t obj_register = Ctx_allocate_register(ctx);
      BC(OP_STORE, obj_register);
      TODO("There is no pg instruction for inserting into an object yet")

      for (size_t i = 0; i < size; i++) {
        Node *member = LIST_get_UNSAFE(&n->children, i);
        compile(alloc, vm, ctx, member);
      }

      // move the array back into r0, since it needs to be the return value of
      // this N_ARRAY and N_LIST node
      BC(OP_LOAD, obj_register);
      Ctx_free_register(ctx, obj_register);
    }
    break;
  }
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
        Node *member = LIST_get_UNSAFE(&n->children, i);
        compile(alloc, vm, ctx, member);
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

Ctx cc(Vm *vm, Allocator *alloc, Parser *p) {
  ByteCodeBuilder bcb = ByteCodeBuilder_new(alloc);

  // specifically set size 1 to keep r0 the temporary register reserved
  Ctx ctx = {
      .register_allocated_count = 1,
      .registers = {0},
      .global_hash_buckets = {0},
      .hash_to_function = {},
      .bcb = &bcb,
      .std = std_tree(vm->config),
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
