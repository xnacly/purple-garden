#include <stdlib.h>

#include "builtins.h"
#include "cc.h"
#include "common.h"
#include "lexer.h"
#include "parser.h"
#include "vm.h"

#define BC(CODE, ARG)                                                          \
  {                                                                            \
    grow_bytecode(vm);                                                         \
    vm->bytecode[vm->bytecode_len++] = CODE;                                   \
    vm->bytecode[vm->bytecode_len++] = ARG;                                    \
  }

// TODO: all of these require extensive benchmarking
#define GROW_FACTOR 2
#define INITIAL_BYTECODE_SIZE 1024
#define INITIAL_GLOBAL_SIZE 128

static void grow_bytecode(Vm *vm) {
  if (vm->bytecode_len + 2 >= vm->bytecode_cap) {
    size_t new_size = vm->bytecode_cap == 0 ? INITIAL_BYTECODE_SIZE
                                            : vm->bytecode_cap * GROW_FACTOR;
    vm->bytecode_cap = new_size;
    vm->bytecode = realloc(vm->bytecode, new_size * sizeof(byte));
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
#if DEBUG
    Token_debug(&t);
#endif
    ASSERT(0, "token_to_value: Unsupported Token.type")
    return (Value){
        .type = V_OPTION,
    };
  }
}

static size_t pool_new(Vm *vm, Value v) {
  // TODO: number interning via custom HashMap for Values, store each global
  // only once - less allocations and less logic
  if (vm->global_len + 1 >= vm->global_cap) {
    size_t new_size = vm->global_cap == 0 ? INITIAL_GLOBAL_SIZE
                                          : vm->global_cap * GROW_FACTOR;
    vm->global_cap = new_size;
    vm->globals = realloc(vm->globals, new_size * sizeof(Value));
  }
  size_t index = vm->global_len;
  vm->globals[index] = v;
  vm->global_len++;
  return index;
}

typedef struct {
  bool registers[REGISTERS + 1];
  size_t size;
  // TODO: keep track of function definition bytecode index here
} Ctx;

static size_t Ctx_allocate_register(Ctx *ctx) {
  ASSERT(ctx->size < REGISTERS, "cc: out of registers")
  ctx->registers[ctx->size] = true;
  return ctx->size++;
}

static void Ctx_free_register(Ctx *ctx, size_t i) {
  ASSERT(i < ctx->size, "cc: register index out of bounds")
  ASSERT(ctx->registers[i], "cc: attempting to free unallocated register")
  ctx->size--;
  ctx->registers[i] = false;
}

static void compile(Vm *vm, Ctx *ctx, Node *n) {
  switch (n->type) {
  case N_ATOM: {
    BC(OP_LOAD, pool_new(vm, token_to_value(n->token)))
    break;
  }
  case N_IDENT: {
    // size_t index = pool_new(vm, token_to_value(n->token));
    // BC(OP_VAR, index);
    TODO("compile#N_IDENT not implemented");
    break;
  }
  case N_LIST: {
    size_t cl = n->children_length;
    if (cl > 0) {
      Node first = n->children[0];
      switch (first.type) {
      case N_BUILTIN:
        String *s = &first.token.string;
        if (String_eq(&STRING("println"), s)) {
          // single argument at r0
          if (cl == 2) {
            compile(vm, ctx, &n->children[1]);
            BC(OP_BUILTIN, BUILTIN_PRINTLN)
          } else {
            TODO("compile#N_BUILTIN for Node.children_length > 3 is not "
                 "implemented");
          }
        } else if (String_eq(&STRING("print"), s)) {
          if (cl == 2) {
            compile(vm, ctx, &n->children[1]);
            BC(OP_BUILTIN, BUILTIN_PRINT)
          } else {
            TODO("compile#N_BUILTIN for Node.children_length > 3 is not "
                 "implemented");
          }
        } else if (String_eq(&STRING("len"), s)) {
          ASSERT(cl == 2, "@len can only be called with a singular argument")
          compile(vm, ctx, &n->children[1]);
          BC(OP_BUILTIN, BUILTIN_LEN)
        } else {
          printf("Unknown builtin: `@");
          String_debug(s);
          puts("`");
          exit(1);
        }
        break;
      case N_IDENT:
        TODO("function calls not implemented");
        break;
      case N_OP:
        byte op;
        switch (first.token.type) {
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
#if DEBUG
          Token_debug(&n->token);
#endif
          ASSERT(0, "Unknown operator")
        }

        // single argument is just a return of that value
        if (cl == 2) {
          compile(vm, ctx, &n->children[1]);
        } else if (cl == 3) {
          // two arguments is easy to compile, just load and add two Values
          compile(vm, ctx, &n->children[1]);
          size_t r = Ctx_allocate_register(ctx);
          BC(OP_STORE, r)
          compile(vm, ctx, &n->children[2]);
          BC(op, r)
          Ctx_free_register(ctx, r);
        } else {
          TODO(
              "compile#N_LIST for Node.children_length > 3 is not implemented");
        }
        break;
      default:
        TODO("compile#N_LIST is not implemented");
        break;
      }
    }
    break;
  }
  default:
    ASSERT(0, "N_UNKOWN is no a known Node to compile, sorry");
    break;
  }
}

Vm cc(Node *n) {
  Vm vm = {.global_len = 0,
           .global_cap = INITIAL_GLOBAL_SIZE,
           .bytecode_len = 0,
           .bytecode_cap = INITIAL_BYTECODE_SIZE,
           .pc = 0,
           .bytecode = NULL,
           .globals = NULL};
  vm.bytecode = malloc(sizeof(byte) * INITIAL_BYTECODE_SIZE);
  vm.globals = malloc(sizeof(Value) * INITIAL_GLOBAL_SIZE);
  // specifically set size 1 to keep r0 the temporary register
  Ctx ctx = {.size = 1, .registers = {0}};

  // we iterate over the children of n, since the parser stores all nodes of an
  // input inside of a root node
  for (size_t i = 0; i < n->children_length; i++) {
    compile(&vm, &ctx, &n->children[i]);
  }

  return vm;
}

String disassemble(const Vm *vm) { return STRING_EMPTY; }

#undef BC
#undef TODO
