#ifndef VM_H
#define VM_H

#include "parser.h"
#include <stdint.h>
#include <stdlib.h>

#define REGISTERS 128
#define CALL_ARGUMENT_STACK 256
#define VARIABLE_TABLE_SIZE 256
#define VARIABLE_TABLE_SIZE_MASK (VARIABLE_TABLE_SIZE - 1)

#define DIS(op, arg)                                                           \
  printf("VM[%06zu][%-8.*s][%10lu]: {.registers=[", vm->pc,                    \
         (int)OP_MAP[(op)].len, OP_MAP[(op)].p, (size_t)arg);                  \
  for (size_t i = 0; i < REGISTERS; i++) {                                     \
    printf(" ");                                                               \
    if (vm->registers[i].type == V_UNDEFINED)                                  \
      break;                                                                   \
    Value_debug(&vm->registers[i]);                                            \
  }                                                                            \
  printf("]");                                                                 \
  if (vm->stack_cur) {                                                         \
    printf(",.stack=[");                                                       \
  }                                                                            \
  for (size_t i = 0; i < vm->stack_cur; i++) {                                 \
    printf(" ");                                                               \
    Value_debug(&vm->stack[i]);                                                \
  }                                                                            \
  if (vm->stack_cur) {                                                         \
    printf(" ]");                                                              \
  }                                                                            \
  printf("}\n");

typedef enum {
  // LOAD rANY
  //
  // LOAD a Value from the const table to r0
  OP_LOAD,
  // STORE rANY
  //
  // STORE a Value from r0 into an arbitrary register
  OP_STORE,
  // OP_VAR globalANY
  //
  // Copy value from Frame assigned to variable name stored in global pool at
  // globalANY
  // OP_VAR,

  // OP_ADD rANY
  //
  // add Value at rANY to r0, store result in r0
  OP_ADD = 2,
  // OP_SUB rANY
  //
  // subtract Value at rANY from r0, store result in r0
  OP_SUB = 3,
  // OP_MUL rANY
  //
  // multiply Value at rANY with r0, store result in r0
  OP_MUL = 4,
  // OP_DIV rANY
  //
  // divide Value at rANY with r0, store result in r0
  OP_DIV = 5,

  // TODO: possibly optimisable by adding a OP_PUSHG for directly pushing a
  // global atom by its index into the vm arg stack

  // OP_PUSH rANY
  //
  // pushes the value in rANY to the stack
  OP_PUSH,

  // OP_POP rANY
  //
  // pops a value from the stack
  OP_POP,

  // OP_VAR rANY
  //
  // stores the Value in the register rANY in the variable table via the
  // identifier stored in r0
  OP_VAR,

  // OP_LOADV hash
  //
  // loads the Value stored in the variable table by hash
  OP_LOADV,

  // OP_ARGS aANY
  //
  // instructs the vm on how many values to pop of the argument stacks
  OP_ARGS,

  // OP_BUILTIN bANY
  //
  // call the builtin its argument refers to, with the argument stored in r0
  OP_BUILTIN,

  // OP_RET rANY
  //
  // returns r0 from the current scope
  OP_RET,

  // OP_CALL ADDR
  //
  // 1 enters a new stackframe, stores the last vm->pc in
  // Frame.return_to_bytecode
  //
  // 2 jumps to ADDR
  OP_CALL,

  // OP_JMP bc
  //
  // Jumps to bc in bytecode index, does no bounds checking
  OP_JMP,
} VM_OP;

extern Str OP_MAP[];

typedef uint32_t byte;

// A frame represents a Scope, a new scope is created upon entering a lambda -
// since lambdas are pure there is no way to interact with the previous frame
// inside of a lambda, the pointer is kept to allow the runtime to restore the
// scope state to its state before entering the lambda
//
// WARNING: do not stack allocate, since variable_table can be huge
typedef struct Frame {
  struct Frame *prev;
  // returning out of scope, we need to jump back to the callsite of the
  // function
  size_t return_to_bytecode;
  // stores Values by their hash, serving as a variable table
  Value variable_table[VARIABLE_TABLE_SIZE];
} Frame;

typedef struct {
  uint32_t global_len;
  // globals represents the global pool created by the bytecode compiler
  Value *globals;
  uint64_t bytecode_len;
  byte *bytecode;
  // current position in the bytecode
  size_t pc;
  Value registers[REGISTERS + 1];
  // frame stores variables of the current scope, meta data and other required
  // data
  Frame *frame;
  // stack is used for handling arguments of a function or builtin call, if more
  // than one arguments are passed to a function these are pushed to the stack,
  // except the last one, which is in r0 either way, so we just take it from
  // there
  Value stack[CALL_ARGUMENT_STACK];
  // stack_cur stores how many elements there currently are in the stack
  size_t stack_cur;
  // arg_count enables the vm to know how many register values it needs to pop
  // off the stack and pass to the function called via CALL or BUILTIN
  size_t arg_count;
} Vm;

int Vm_run(Vm *vm, Allocator *alloc);
void Vm_destroy(Vm *vm);
void Value_debug(const Value *v);

#endif
