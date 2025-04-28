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

#ifndef DISASSEMBLE_INCLUDE_POSITIONS
#define DISASSEMBLE_INCLUDE_POSITIONS 0
#endif

#define BC(CODE, ARG)                                                          \
  vm->bytecode[vm->bytecode_len++] = CODE;                                     \
  vm->bytecode[vm->bytecode_len++] = ARG;                                      \
  ASSERT(vm->bytecode_len <= BYTECODE_SIZE,                                    \
         "cc: out of bytecode space, what the fuck are you doing (there is "   \
         "space for 4MB of bytecode)");

#define GROW_FACTOR 2
#define MAX_BUILTIN_SIZE 1024
#define MAX_BUILTIN_SIZE_MASK (MAX_BUILTIN_SIZE - 1)

typedef enum {
  COMPILE_BUILTIN_LET = 256,
  COMPILE_BUILTIN_FUNCTION,
} COMPILE_BUILTIN;

void disassemble(const Vm *vm, const Ctx *ctx) {
  puts("; vim: filetype=asm");
  printf("; Vm {global=%u/%d, bytecode=%zu/%d}\n", vm->global_len, GLOBAL_SIZE,
         vm->bytecode_len, BYTECODE_SIZE);
  if (vm->global_len > 0) {
    printf("__globals:\n\t");
    for (size_t i = 0; i < vm->global_len; i++) {
      Value *v = &vm->globals[i];
      Value_debug(v);
      printf("; {idx=%zu", i);
      if (v->type == V_STR) {
        printf(",hash=%zu", v->string.hash & GLOBAL_MASK);
      }
      printf("}\n\t");
    }
  }

  bool ctx_available = ctx != NULL;

  if (vm->bytecode_len > 0) {
    printf("\n__entry:");
    for (size_t i = 0; i < vm->bytecode_len; i += 2) {
      if (ctx_available) {
        for (size_t j = 0; j < MAX_BUILTIN_SIZE; j++) {
          size_t location = ctx->function_hash_to_bytecode_index[j];
          if (location == i) {
            if (location != 0) {
              puts("");
            }
            printf("\n__0x%06zX[%04zX]:", i, j);
          }
        }
      }
      VM_OP op = vm->bytecode[i];
      size_t arg = vm->bytecode[i + 1];
#if DISASSEMBLE_INCLUDE_POSITIONS
      printf("\n\t; @0x%04zX/0x%04zX", i, i + 1);
#endif
      printf("\n\t");
      Str_debug(&OP_MAP[op]);

      // dont print the argument if its unused in the vm
      switch (op) {
      case OP_RET:
        puts("");
      case OP_POP:
        break;
#if DISASSEMBLE_INCLUDE_POSITIONS
      case OP_JMP:
        printf(" 0x%04zX", arg);
        break;
#endif
      default:
        printf(" %zu", arg);
      }

      switch (op) {
      case OP_LOAD:
        printf(": ");
        Value_debug(&vm->globals[arg]);
        break;
      case OP_BUILTIN:
        printf(": <@%.*s>", (int)BUILTIN_NAME_MAP[arg].len,
               BUILTIN_NAME_MAP[arg].p);
        break;
      case OP_CALL: {
        printf(": <%04zX>", arg);
        break;
      }
      default:
        break;
      }
    }
  }
}

// token_to_value converts primitive tokens, such as strings, boolean and
// numbers to runtime values
static Value token_to_value(Token t) {
  switch (t.type) {
  case T_STRING:
  case T_IDENT:
    return (Value){.type = V_STRING, .string = t.string};
  case T_TRUE:
    return (Value){.type = V_TRUE};
  case T_FALSE:
    return (Value){.type = V_FALSE};
  case T_NUMBER:
    return (Value){.type = V_NUM, .number = t.number};
  default:
    // TODO: think about lists and options
    ASSERT(0, "Unsupported value for this")
    return (Value){
        .type = V_UNDEFINED,
    };
  }
}

typedef struct {
  bool registers[REGISTERS + 1];
  size_t *global_hash_buckets;
  size_t size;
  // TODO: keep track of function definition bytecode index here
} Ctx;

static size_t Ctx_allocate_register(Ctx *ctx) {
  ASSERT(ctx->size < REGISTERS, "cc: out of registers")
  ctx->registers[ctx->size] = true;
  return ctx->size++;
}

static void Ctx_free_register(Ctx *ctx, size_t i) {
  assert(i < ctx->size && "cc: register index out of bounds");
  assert(ctx->registers[i] && "cc: attempting to free unallocated register");
  ctx->size--;
  ctx->registers[i] = false;
}

static size_t runtime_builtin_hashes[MAX_BUILTIN_SIZE + 1];

static void compile(Allocator *alloc, Vm *vm, Ctx *ctx, Node *n) {
  switch (n->type) {
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
        vm->globals[vm->global_len++] = token_to_value(*n->token);
      }
      BC(OP_LOAD, expected_index)
    } else {
      ASSERT(vm->global_len + 1 < GLOBAL_SIZE,
             "cc: out of global space, what the fuck are you doing (there is "
             "space "
             "for 256k globals)");
      vm->globals[vm->global_len] = token_to_value(*n->token);
      BC(OP_LOAD, vm->global_len++)
    }
    break;
  }
  case N_IDENT: {
    BC(OP_LOADV, n->token->string.hash & GLOBAL_MASK);
    break;
  }
  case N_OP: {
    // assumes lexer did its work correctly
    byte op = ((byte[]){
        [T_PLUS] = OP_ADD,
        [T_MINUS] = OP_SUB,
        [T_ASTERISKS] = OP_MUL,
        [T_SLASH] = OP_DIV,
    }[n->token->type]);

    // single argument is just a return of that value
    if (n->children_length == 1) {
      compile(alloc, vm, ctx, n->children[0]);
    } else if (n->children_length == 2) {
      // two arguments is easy to compile, just load and add two Values
      compile(alloc, vm, ctx, n->children[0]);
      size_t r = Ctx_allocate_register(ctx);
      BC(OP_STORE, r)
      compile(alloc, vm, ctx, n->children[1]);
      BC(op, r)
      Ctx_free_register(ctx, r);
    } else {
      TODO("compile#N_LIST for Node.children_length > 3 is not implemented");
    }
    break;
  }
  case N_BUILTIN: {
    if (!n->children_length) {
      // NOTE: skip generating bytecode for empty builtin invocations
      return;
    }

    Str *s = &n->token->string;
    int b = runtime_builtin_hashes[s->hash & MAX_BUILTIN_SIZE_MASK];
    ASSERT(b != BUILTIN_UNKOWN, "Unknown builtin `@%.*s`", (int)s->len, s->p)

    // compile time pseudo builtins
    switch (b) {
    case COMPILE_BUILTIN_LET: // @len <var-name> <var-value>
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
        ASSERT(vm->global_len + 1 < GLOBAL_SIZE,
               "cc: out of global space, what the fuck are you doing (there is "
               "space "
               "for 256k globals)");
        ctx->global_hash_buckets[hash] = vm->global_len + 1;
        vm->globals[vm->global_len++] =
            (Value){.type = V_STRING, .string = ident->string};
      }
      BC(OP_LOAD, expected_index);
      BC(OP_VAR, r);
      Ctx_free_register(ctx, r);
      break;
    default:
      // single argument at r0
      if (n->children_length == 1) {
        compile(alloc, vm, ctx, n->children[0]);
        // PERF: removed BC(OP_ARGS, 1), since a singular argument to a builtin
        // is the default optimized branch
      } else {
        for (size_t i = 0; i < n->children_length; i++) {
          compile(alloc, vm, ctx, n->children[i]);
          if (i < n->children_length - 1) {
            BC(OP_PUSH, 0)
          }
        }

        BC(OP_ARGS, n->children_length);
      }

      BC(OP_BUILTIN, b);
      break;
    }
    break;
  }
  default:
    ASSERT(0, "N_UNKOWN is no a known Node to compile, sorry");
    break;
  }
}

Vm cc(Allocator *alloc, Node **nodes, size_t size) {
  // runtime functions
  runtime_builtin_hashes[Str_hash(&STRING("println")) & MAX_BUILTIN_SIZE_MASK] =
      BUILTIN_PRINTLN;
  runtime_builtin_hashes[Str_hash(&STRING("print")) & MAX_BUILTIN_SIZE_MASK] =
      BUILTIN_PRINT;
  runtime_builtin_hashes[Str_hash(&STRING("len")) & MAX_BUILTIN_SIZE_MASK] =
      BUILTIN_LEN;
  runtime_builtin_hashes[Str_hash(&STRING("type")) & MAX_BUILTIN_SIZE_MASK] =
      BUILTIN_TYPE;

  // compile time constructs
  runtime_builtin_hashes[Str_hash(&STRING("let")) & MAX_BUILTIN_SIZE_MASK] =
      COMPILE_BUILTIN_LET;

  Vm vm = {
      .global_len = 0,
      .bytecode_len = 0,
      .pc = 0,
      .bytecode = NULL,
      .globals = NULL,
      .stack = {{0}},
      .stack_cur = 0,
  };
  vm.bytecode = alloc->request(alloc->ctx, (sizeof(byte) * BYTECODE_SIZE));
  vm.globals = alloc->request(alloc->ctx, (sizeof(Value) * GLOBAL_SIZE));
  vm.globals[0] = (Value){.type = V_FALSE};
  vm.globals[1] = (Value){.type = V_TRUE};
  vm.global_len += 2;
  // specifically set size 1 to keep r0 the temporary register reserved
  Ctx ctx = {.size = 1, .registers = {0}};
  ctx.global_hash_buckets =
      alloc->request(alloc->ctx, sizeof(Value) * GLOBAL_SIZE);

  for (size_t i = 0; i < size; i++) {
    compile(alloc, &vm, &ctx, nodes[i]);
  }
  return vm;
}

#undef BC
#undef TODO
