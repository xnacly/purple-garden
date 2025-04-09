#include "assert.h"
#include <stdlib.h>

#include "builtins.h"
#include "cc.h"
#include "common.h"
#include "lexer.h"
#include "lookup.h"
#include "mem.h"
#include "parser.h"
#include "strings.h"
#include "vm.h"

#define BC(CODE, ARG)                                                          \
  vm->bytecode[vm->bytecode_len++] = CODE;                                     \
  vm->bytecode[vm->bytecode_len++] = ARG;                                      \
  ASSERT(vm->bytecode_len <= BYTECODE_SIZE,                                    \
         "cc: out of bytecode space, what the fuck are you doing (there is "   \
         "space for 4MB of bytecode)");

// TODO: all of these require extensive benchmarking
#define GROW_FACTOR 2

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
#if DEBUG
    Token_debug(&t);
#endif
    return (Value){
        .type = V_UNDEFINED,
    };
  }
}

typedef struct {
  bool registers[REGISTERS + 1];
  size_t global_hash_buckets[GLOBAL_SIZE];
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

static void compile(Allocator *alloc, Vm *vm, Ctx *ctx, Node *n) {
  switch (n->type) {
  case N_ATOM: {
    // interning only for Strings
    if (n->token.type == T_STRING) {
      size_t hash = Str_hash(&n->token.string);
      size_t cached_index = ctx->global_hash_buckets[hash];
      size_t expected_index = vm->global_len;
      if (cached_index) {
        expected_index = cached_index - 1;
      } else {
        ctx->global_hash_buckets[hash] = vm->global_len + 1;
        vm->globals[vm->global_len++] = token_to_value(n->token);
      }
      BC(OP_LOAD, expected_index)
    } else {
      vm->globals[vm->global_len] = token_to_value(n->token);
      ASSERT(vm->global_len <= GLOBAL_SIZE,
             "cc: out of global space, what the fuck are you doing (there is "
             "space "
             "for 256k globals)");
      BC(OP_LOAD, vm->global_len++)
    }
    break;
  }
  case N_IDENT: {
    // size_t index = pool_new(vm, token_to_value(n->token));
    // BC(OP_VAR, index);
    TODO("compile#N_IDENT not implemented");
    break;
  }
  case N_OP: {
    byte op;
    switch (n->token.type) {
    case T_PLUS:
      op = OP_ADD;
      break;
    case T_MINUS:
      op = OP_SUB;
      break;
    case T_ASTERISKS:
      op = OP_MUL;
      break;
    case T_SLASH:
      op = OP_DIV;
      break;
    default:
      ASSERT(0, "Unknown operator")
    }

    // single argument is just a return of that value
    if (n->children_length == 1) {
      compile(alloc, vm, ctx, &n->children[0]);
    } else if (n->children_length == 2) {
      // two arguments is easy to compile, just load and add two Values
      compile(alloc, vm, ctx, &n->children[0]);
      size_t r = Ctx_allocate_register(ctx);
      BC(OP_STORE, r)
      compile(alloc, vm, ctx, &n->children[1]);
      BC(op, r)
      Ctx_free_register(ctx, r);
    } else {
      TODO("compile#N_LIST for Node.children_length > 3 is not implemented");
    }
    break;

    break;
  }
  case N_BUILTIN: {
    Str s = n->token.string;

    BUILTIN_LOOKUP

    // single argument at r0
    if (n->children_length == 1) {
      compile(alloc, vm, ctx, &n->children[0]);
      BC(OP_BUILTIN, b)
    } else {
      TODO("compile#N_BUILTIN for Node.children_length > 3 is not "
           "implemented");
    }
    break;
  }
  default:
    ASSERT(0, "N_UNKOWN is no a known Node to compile, sorry");
    break;
  }
}

Vm cc(Parser *p) {
  Vm vm = {.global_len = 0,
           .bytecode_len = 0,
           .pc = 0,
           .bytecode = NULL,
           .globals = NULL};
  vm.bytecode =
      p->alloc->request(p->alloc->ctx, (sizeof(byte) * BYTECODE_SIZE));
  vm.globals = p->alloc->request(p->alloc->ctx, (sizeof(Value) * GLOBAL_SIZE));
  // specifically set size 1 to keep r0 the temporary register
  Ctx ctx = {.size = 1, .registers = {0}, .global_hash_buckets = {0}};
#if DEBUG
  puts("=================  AST  =================");
#endif
  while (p->cur.type != T_EOF) {
    Node n = Parser_next(p);
#if DEBUG
    Node_debug(&n, 0);
    puts("");
#endif
    if (n.type != N_UNKOWN) {
      compile(p->alloc, &vm, &ctx, &n);
    }
  }
  return vm;
}

void disassemble(const Vm *vm) {
  puts("; vim: filetype=asm");
  printf("; Vm {global=%zu/%d, bytecode=%zu/%d}\n", vm->global_len, GLOBAL_SIZE,
         vm->bytecode_len, BYTECODE_SIZE);
  if (vm->global_len > 0) {
    printf("globals:\n\t");
    for (size_t i = 0; i < vm->global_len; i++) {
      Value *v = &vm->globals[i];
      Value_debug(v);
      printf("; {idx=%zu}\n\t", i);
    }
  }
  puts("\nentry: ");
  if (vm->bytecode_len > 0) {
    for (size_t i = 0; i < vm->bytecode_len; i += 2) {
      VM_OP op = vm->bytecode[i];
      size_t arg = vm->bytecode[i + 1];
      printf("\t; [op=%d,arg=%zu] at (%zu/%zu)", op, arg, i, i + 1);
      switch (op) {
      case OP_LOAD:
        printf("\n\t; global=");
        Value_debug(&vm->globals[arg]);
        break;
      case OP_BUILTIN:
        printf("\n\t; builtin=@");
        Str_debug(&BUILTIN_NAME_MAP[arg]);
        break;
      default:
        break;
      }
      printf("\n\t");
      Str_debug(&OP_MAP[op]);
      printf(" %zu\n", arg);
    }
  }
}

#undef BC
#undef TODO
