#include <stdlib.h>

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
  case T_BOOLEAN:
    return (Value){.type = t.boolean ? V_TRUE : V_FALSE};
  case T_NUMBER:
    return (Value){.type = V_NUM, .number = t.number};
  default:
#if DEBUG
    Token_debug(&t);
#endif
    ASSERT(0, "token_to_value: Unsupported Token.type")
    return (Value){.type = V_NULL};
  }
}

static size_t pool_new(Vm *vm, Value v) {
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

static void compile(Vm *vm, Node *n) {
  switch (n->type) {
  case N_ATOM: {
    size_t index = pool_new(vm, token_to_value(n->token));
    BC(OP_LOAD, index)
    break;
  }
  case N_IDENT: {
    // size_t index = pool_new(vm, token_to_value(n->token));
    // BC(OP_VAR, index);
    TODO("compile#N_IDENT not implemented");
    break;
  }
  case N_LIST: {
    if (n->children_length) {
      Node first = n->children[0];
      switch (first.type) {
      case N_IDENT:
        TODO("function calls not implemented");
        break;
      case N_OP:
        TODO("compile#N_OP unimplemented");
        break;
      case N_UNKOWN:
      default:
        TODO("compile#N_LIST is not implemented");
        break;
      }
    }
    break;
  }
  case N_UNKOWN:
  default:
    TODO("N_UNKOWN is no a known Node to compile, sorry");
    break;
  }
}

Vm cc(Node *n) {
  Vm vm = {.global_len = 0,
           .global_cap = INITIAL_GLOBAL_SIZE,
           .bytecode_len = 0,
           .bytecode_cap = INITIAL_BYTECODE_SIZE,
           ._pc = 0,
           .bytecode = NULL,
           .globals = NULL};
  vm.bytecode = malloc(sizeof(byte) * INITIAL_BYTECODE_SIZE);
  vm.globals = malloc(sizeof(Value) * INITIAL_GLOBAL_SIZE);

  // we iterate over the children of n, since the parser stores all nodes of an
  // input inside of a root node
  for (size_t i = 0; i < n->children_length; i++) {
    compile(&vm, &n->children[i]);
  }

  return vm;
}

#undef BC
#undef TODO
