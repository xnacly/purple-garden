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

static size_t hashes[4];

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
      size_t hash = n->token->string.hash;
      size_t cached_index = ctx->global_hash_buckets[hash];
      size_t expected_index = vm->global_len;
      if (cached_index) {
        expected_index = cached_index - 1;
      } else {
        ASSERT(vm->global_len <= GLOBAL_SIZE,
               "cc: out of global space, what the fuck are you doing (there is "
               "space "
               "for 256k globals)");
        ctx->global_hash_buckets[hash] = vm->global_len + 1;
        vm->globals[vm->global_len++] = token_to_value(*n->token);
      }
      BC(OP_LOAD, expected_index)
    } else {
      ASSERT(vm->global_len <= GLOBAL_SIZE,
             "cc: out of global space, what the fuck are you doing (there is "
             "space "
             "for 256k globals)");
      vm->globals[vm->global_len] = token_to_value(*n->token);
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
    switch (n->token->type) {
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
  }
  case N_BUILTIN: {
    if (!n->children_length) {
      // PERF: skip generating bytecode for empty builtin invocations
      return;
    }
    Builtin b = BUILTIN_UNKOWN;
    Str *s = &n->token->string;
    if (s->hash == hashes[BUILTIN_LEN]) {
      b = BUILTIN_LEN;
    } else if (s->hash == hashes[BUILTIN_PRINT]) {
      b = BUILTIN_PRINT;
    } else if (s->hash == hashes[BUILTIN_PRINTLN]) {
      b = BUILTIN_PRINTLN;
    }
    ASSERT(b != BUILTIN_UNKOWN, "Unknown builtin at this point...")

    size_t registers[n->children_length > 1 ? n->children_length - 1 : 1];
    // single argument at r0
    if (n->children_length == 1) {
      compile(alloc, vm, ctx, &n->children[0]);
      BC(OP_ARGS, 1);
    } else {
      int offset = ctx->size - 1;
      for (size_t i = 0; i < n->children_length; i++) {
        compile(alloc, vm, ctx, &n->children[i]);
        if (i < n->children_length - 1) {
          size_t r = Ctx_allocate_register(ctx);
          BC(OP_STORE, r)
          registers[i] = r;
        }
      }

      // TODO: pack ARGS and OFFSET into a ARGOFF bytecode via bytepacking,
      // lower 4 bits for the former and higher 4 bits for the latter
      //
      // packing:
      //
      // uint8_t operand =
      //    ((offset & 0x0F) << 4) | (n->children_length & 0x0F)
      //
      // unpacking:
      //
      // uint8_t num_args = operand & 0x0F;
      // uint8_t offset = (operand >> 4) & 0x0F;
      //
      // This would result in limiting both length and offset to 0-15, should be
      // fine, normally it would be 256, i could also use 3 bytes as the length
      // and 5 bytes as the offset, resulting in not 16/16 limits but 8/32, that
      // should be better, since no functions should really even have 8
      // arguments, thats a smell.
      BC(OP_OFFSET, offset);
      BC(OP_ARGS, n->children_length);
    }

    BC(OP_BUILTIN, b);
    // only deallocate registers if we have any allocated
    if (n->children_length > 1) {
      // skip last because we dont store the last in a specific register, free
      // others
      for (int i = n->children_length - 2; i > -1; i--) {
        Ctx_free_register(ctx, registers[i]);
      }
    }

    break;
  }
  default:
    ASSERT(0, "N_UNKOWN is no a known Node to compile, sorry");
    break;
  }
}

Vm cc(Allocator *alloc, Node *nodes, size_t size) {
  hashes[BUILTIN_PRINTLN] = Str_hash(&STRING("println"));
  hashes[BUILTIN_PRINT] = Str_hash(&STRING("print"));
  hashes[BUILTIN_LEN] = Str_hash(&STRING("len"));

  Vm vm = {.global_len = 0,
           .bytecode_len = 0,
           .pc = 0,
           .bytecode = NULL,
           .globals = NULL};
  vm.bytecode = alloc->request(alloc->ctx, (sizeof(byte) * BYTECODE_SIZE));
  vm.globals = alloc->request(alloc->ctx, (sizeof(Value) * GLOBAL_SIZE));
  vm.globals[0] = (Value){.type = V_FALSE};
  vm.globals[1] = (Value){.type = V_TRUE};
  vm.global_len += 2;
  // specifically set size 1 to keep r0 the temporary register
  Ctx ctx = {.size = 1, .registers = {0}};
  ctx.global_hash_buckets = alloc->request(alloc->ctx, GLOBAL_SIZE);

  for (size_t i = 0; i < size; i++) {
    Node n = nodes[i];
    compile(alloc, &vm, &ctx, &n);
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
      printf("; {idx=%zu", i);
      if (v->type == V_STRING) {
        printf(",hash=%zu", v->string.hash);
      }
      printf("}\n\t");
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
