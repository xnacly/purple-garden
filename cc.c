#include "assert.h"
#include <stdlib.h>

#include "builtins.h"
#include "cc.h"
#include "common.h"
#include "lexer.h"
#include "mem.h"
#include "parser.h"
#include "strings.h"
#include "vm.h"

// token_to_value converts tokens, such as strings, boolean and numbers to
// runtime values
static Value *token_to_value(Token *t, Allocator *a) {
  Value *v = a->request(a->ctx, sizeof(Value));
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
    v->integer = t->integer;
    break;
  case T_DOUBLE:
    v->type = V_DOUBLE;
    v->floating = t->floating;
    break;
  case T_BRAKET_LEFT: // TODO: think about lists
    v->type = V_ARRAY;
    v->array = (struct Array){.len = 0};
    break;
  default:
    ASSERT(0, "Unsupported value for this")
    break;
  }
  return v;
}

static size_t Ctx_allocate_register(Ctx *ctx) {
  ASSERT(ctx->register_allocated_count < REGISTERS, "cc: out of registers")
  ctx->registers[ctx->register_allocated_count] = true;
  return ctx->register_allocated_count++;
}

static void Ctx_free_register(Ctx *ctx, size_t i) {
  assert(i < ctx->register_allocated_count &&
         "cc: register index out of bounds");
  assert(ctx->registers[i] && "cc: attempting to free unallocated register");
  ctx->register_allocated_count--;
  ctx->registers[i] = false;
}

static size_t runtime_builtin_hashes[MAX_BUILTIN_SIZE + 1];

static void compile(Allocator *alloc, Vm *vm, Ctx *ctx, Node *n) {
  switch (n->type) {
  case N_ARRAY: {
    if (n->children_length == 0) {
      ASSERT(vm->global_len + 1 < GLOBAL_SIZE,
             "cc: out of global space, what the fuck are you doing (there is "
             "space "
             "for 256k globals)");
      vm->globals[vm->global_len] = token_to_value(n->token, alloc);
      BC(OP_LOAD, vm->global_len++)
    } else {
      TODO("N_ARRAY#real arrays")
    }
    break;
  }
  case N_ATOM: {
    // interning logic, global pool 0 is the only instance for false in the
    // runtime, 1 for true, strings get interned by their hashes
    if (n->token->type == T_FALSE) {
      BC(OP_LOAD, 0)
    } else if (n->token->type == T_TRUE) {
      BC(OP_LOAD, 1)
    } else if (n->token->type == T_STRING) {
      size_t hash = n->token->string.hash & GLOBAL_MASK;
      size_t cached_index = ctx->global_hash_buckets[hash];
      size_t expected_index = vm->global_len;
      if (cached_index) {
        expected_index = cached_index - 1;
      } else {
        ASSERT(vm->global_len + 1 < GLOBAL_SIZE,
               "cc: out of global space, what the fuck are you doing (there is "
               "space "
               "for 256k globals)");
        ctx->global_hash_buckets[hash] = vm->global_len + 1;
        vm->globals[vm->global_len++] = token_to_value(n->token, alloc);
      }
      BC(OP_LOAD, expected_index)
    } else {
      ASSERT(vm->global_len + 1 < GLOBAL_SIZE,
             "cc: out of global space, what the fuck are you doing (there is "
             "space "
             "for 256k globals)");
      vm->globals[vm->global_len] = token_to_value(n->token, alloc);
      BC(OP_LOAD, vm->global_len++)
    }
    break;
  }
  case N_IDENT: {
    uint64_t hash = n->token->string.hash & GLOBAL_MASK;
    ASSERT(ctx->global_hash_buckets[hash], "Undefined variable `%.*s`",
           (int)n->token->string.len, n->token->string.p);
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
    if (!n->children_length) {
      // TODO: is this really what we want? What about an empty println, this
      // should just call puts
      //
      // skip generating bytecode for empty builtin invocations
      return;
    }

    Str *s = &n->token->string;
    int b = runtime_builtin_hashes[s->hash & MAX_BUILTIN_SIZE_MASK];
    // compile time "pseudo" builtins
    if (b != 0) {
      switch (b) {
      case COMPILE_BUILTIN_FUNCTION: { // (@function <name> [<args>] <body>)
        ASSERT(n->children_length >= 2,
               "@function expects <name> [<arguments>] [body]");
        Str *name = &n->children[0]->token->string;
        size_t hash = name->hash & MAX_BUILTIN_SIZE_MASK;
        ctx->function_hash_to_bytecode_index[hash] = vm->bytecode_len;
        // TODO: replace this with the new annotation systems
        ctx->function_hash_to_function_name[hash] =
            n->children[0]->token->string;

        // this is the worst hack i have ever written, this is used to jump over
        // the bytecode of a function (header with args setup and body), so we
        // keep the bytecode compilation single pass and the bytecode linear,
        // this works (for now at least)
        size_t jump_op_index = vm->bytecode_len;
        BC(OP_JMP,
           0xAFFEDEAD); // https://de.wiktionary.org/wiki/Klappe_zu,_Affe_tot

        Node **params = n->children[1]->children;
        size_t param_len = n->children[1]->children_length;

        // Calling convention overview:
        //
        // parameters=[a b c]
        // arguments=[0 1 2]
        // stack=[0 1]
        //
        // r0 = c
        // stack top = b
        // stack top-1 = a

        // calling convention mandates accumulator register (r0) holds the value
        // for the last argument of a function, thus we need to move it to its
        // variable
        if (param_len > 0) {
          Node *param = params[param_len - 1];
          ASSERT(param->type == N_IDENT,
                 "Expected identifier as function parameter in `%.*s` "
                 "definition, got `%.*s`",
                 (int)name->len, name->p, (int)NODE_TYPE_MAP[param->type].len,
                 NODE_TYPE_MAP[param->type].p);

          size_t r = Ctx_allocate_register(ctx);
          BC(OP_STORE, r);
          size_t param_hash = param->token->string.hash & GLOBAL_MASK;
          size_t cached_index = ctx->global_hash_buckets[param_hash];
          size_t expected_index = vm->global_len;
          if (cached_index) {
            // -1 because index is stored +1 to distinguish unset (0) from index
            // 0
            expected_index = cached_index - 1;
          } else {
            ASSERT(
                vm->global_len + 1 < GLOBAL_SIZE,
                "cc: out of global space, what the fuck are you doing (there "
                "is space for 256k globals)");
            // stored +1 to distinguish unset (0) from index 0
            ctx->global_hash_buckets[param_hash] = vm->global_len + 1;
            vm->globals[vm->global_len++] = token_to_value(param->token, alloc);
          }

          BC(OP_LOAD, expected_index);
          BC(OP_VAR, r)
          Ctx_free_register(ctx, r);
        }

        // calling convention mandates all arguments > 1 to be on the stack, we
        // deal with this here
        if (param_len > 1) {
          for (int i = param_len - 2; i > -1; i--) {
            Node *param = params[i];
            ASSERT(param->type == N_IDENT,
                   "Expected identifier as function parameter in `%.*s` "
                   "definition, got `%.*s`",
                   (int)name->len, name->p, (int)NODE_TYPE_MAP[param->type].len,
                   NODE_TYPE_MAP[param->type].p);
            BC(OP_POP, 0);
            size_t r = Ctx_allocate_register(ctx);
            BC(OP_STORE, r);
            size_t param_hash = param->token->string.hash & GLOBAL_MASK;
            size_t cached_index = ctx->global_hash_buckets[param_hash];
            size_t expected_index = vm->global_len;
            if (cached_index) {
              expected_index = cached_index - 1;
            } else {
              ASSERT(vm->global_len + 1 < GLOBAL_SIZE,
                     "cc: out of global space, what the fuck are you doing "
                     "(there is space for 256k globals)");
              ctx->global_hash_buckets[param_hash] = vm->global_len + 1;
              vm->globals[vm->global_len++] =
                  token_to_value(param->token, alloc);
            }

            BC(OP_LOAD, expected_index);
            BC(OP_VAR, r)
            Ctx_free_register(ctx, r);
          }
        }

        // compiling the body, returning a value is free since its just in r0
        if (n->children_length > 2) {
          for (size_t i = 2; i < n->children_length; i++) {
            // PERF: if last Node is N_CALL think about reusing call frames
            compile(alloc, vm, ctx, n->children[i]);
          }
        }

        vm->bytecode[jump_op_index + 1] = vm->bytecode_len;
        BC(OP_LEAVE, 0);
        break;
      }
      case COMPILE_BUILTIN_LET: { // (@len <var-name> <var-value>)
        ASSERT(n->children_length == 2,
               "@let requires two arguments: `@let "
               "<var-name> <var-value>`, got %zu",
               n->children_length);
        compile(alloc, vm, ctx, n->children[1]);
        size_t r = Ctx_allocate_register(ctx);
        BC(OP_STORE, r);
        Token *ident = n->children[0]->token;
        size_t hash = ident->string.hash & GLOBAL_MASK;
        size_t cached_index = ctx->global_hash_buckets[hash];
        size_t expected_index = vm->global_len;
        if (cached_index) {
          expected_index = cached_index - 1;
        } else {
          ASSERT(
              vm->global_len + 1 < GLOBAL_SIZE,
              "cc: out of global space, what the fuck are you doing (there is "
              "space "
              "for 256k globals)");
          ctx->global_hash_buckets[hash] = vm->global_len + 1;
          vm->globals[vm->global_len++] = token_to_value(ident, alloc);
        }
        BC(OP_LOAD, expected_index);
        BC(OP_VAR, r);
        Ctx_free_register(ctx, r);
        break;
      }
      default:
      }
    } else {
      size_t hash = s->hash & MAX_BUILTIN_SIZE_MASK;
      builtin_function bf = vm->builtins[hash];
      ASSERT(bf != NULL, "Unknown builtin `@%.*s`", (int)s->len, s->p)

      // single argument at r0
      if (n->children_length == 1) {
        compile(alloc, vm, ctx, n->children[0]);
      } else {
        for (size_t i = 0; i < n->children_length; i++) {
          compile(alloc, vm, ctx, n->children[i]);
          if (i < n->children_length - 1) {
            BC(OP_PUSH, 0)
          }
        }

        BC(OP_ARGS, n->children_length);
      }

      BC(OP_BUILTIN, hash);
    }
    break;
  }
  case N_CALL: {
    Str *name = &n->token->string;
    int loc = ctx->function_hash_to_bytecode_index[name->hash &
                                                   MAX_BUILTIN_SIZE_MASK];
    ASSERT(loc > -1, "Undefined function `%.*s`", (int)name->len, name->p)
    // single argument at r0
    if (n->children_length == 1) {
      compile(alloc, vm, ctx, n->children[0]);
    } else if (n->children_length > 1) {
      for (size_t i = 0; i < n->children_length; i++) {
        compile(alloc, vm, ctx, n->children[i]);
        if (i < n->children_length - 1) {
          BC(OP_PUSH, 0)
        }
      }

      BC(OP_ARGS, n->children_length);
    }

    BC(OP_CALL, loc);
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

Ctx cc(Vm *vm, Allocator *alloc, Node **nodes, size_t size) {
  // compile time constructs
  runtime_builtin_hashes[Str_hash(&STRING("let")) & MAX_BUILTIN_SIZE_MASK] =
      COMPILE_BUILTIN_LET;
  runtime_builtin_hashes[Str_hash(&STRING("function")) &
                         MAX_BUILTIN_SIZE_MASK] = COMPILE_BUILTIN_FUNCTION;

  // specifically set size 1 to keep r0 the temporary register reserved
  Ctx ctx = {
      .register_allocated_count = 1,
      .registers = {0},
      .global_hash_buckets =
          alloc->request(alloc->ctx, sizeof(Value) * GLOBAL_SIZE),
      .function_hash_to_bytecode_index =
          alloc->request(alloc->ctx, sizeof(size_t) * MAX_BUILTIN_SIZE),
      .function_hash_to_function_name =
          alloc->request(alloc->ctx, sizeof(Str) * MAX_BUILTIN_SIZE),
  };

#pragma GCC unroll 64
  for (size_t i = 0; i < MAX_BUILTIN_SIZE; i++) {
    ctx.function_hash_to_bytecode_index[i] = -1;
  }

  for (size_t i = 0; i < size; i++) {
    compile(alloc, vm, &ctx, nodes[i]);
  }
  return ctx;
}

#undef BC
#undef TODO
