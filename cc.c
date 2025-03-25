#include "cc.h"
#include "common.h"
#include "vm.h"
#include <stdlib.h>

#define TODO(msg) ASSERT(0, msg)
#define BC(CODE, ARG)                                                          \
  {                                                                            \
    /*TODO: treat bytecode as an arraylist to minimize allocations */          \
    vm->bytecode =                                                             \
        realloc(vm->bytecode, (vm->bytecode_len + 2) * sizeof(byte));          \
    vm->bytecode[vm->bytecode_len++] = CODE;                                   \
    vm->bytecode[vm->bytecode_len++] = ARG;                                    \
  }

// token_to_value converts primitive tokens, such as strings, boolean and
// numbers to runtime values
static Value token_to_value(Token t) {
  switch (t.type) {
  case T_STRING:
    return (Value){.type = V_STRING, .string = t.string};
  case T_BOOLEAN:
    return (Value){.type = t.boolean ? V_TRUE : V_FALSE};
  case T_NUMBER:
    return (Value){.type = V_NUM, .number = t.number};
  default:
    Token_debug(&t);
    ASSERT(0, "token_to_value: Unsupported Token.type")
    return (Value){.type = V_NULL};
  }
}

static size_t pool_new(Vm *vm, Value v) {
  // TODO: treat pool as an array list to minimize allocations
  size_t index = vm->global_len;
  vm->globals = realloc(vm->globals, (vm->global_len + 1) * sizeof(Value));
  vm->globals[index] = v;
  vm->global_len++;
  return index;
}

static void compile(Vm *vm, Node *n) {
  // INFO: this is an example for a simple simple interaction the compiler could
  // do
  //
  // vm->globals = malloc(sizeof(Value));
  // vm->globals[0] = (Value){.type = V_STRING, .string = STRING("hello
  // world")}; vm->bytecode = malloc(sizeof(byte) * 4); BC(OP_LOAD, 0)
  // BC(OP_STORE, 25)

  switch (n->type) {
  case N_ATOM:
    size_t index = pool_new(vm, token_to_value(n->token));
    BC(OP_LOAD, index)
    break;
  case N_IDENT:
    TODO("N_IDENT is not implemented");
  case N_LIST:
    TODO("N_LIST is not implemented");
  case N_LAMBDA:
    TODO("N_LAMBDA is not implemented");
  case N_OP:
    TODO("N_OP is not implemented");
  case N_UNKOWN:
  default:
    TODO("N_UNKOWN is no a known Node to compile, sorry");
    break;
  }
}

Vm cc(Node *n) {
  Vm vm = {.global_len = 0,
           .bytecode_len = 0,
           ._pc = 0,
           .bytecode = NULL,
           .globals = NULL};
  // we iterate over the children of n, since the parser stores all nodes of an
  // input inside of a root node
  for (size_t i = 0; i < n->children_length; i++) {
    compile(&vm, &n->children[i]);
  }

  return vm;
}

#undef BC
#undef TODO
