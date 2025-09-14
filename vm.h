#pragma once

#include "adts.h"
#include "common.h"
#include <stdint.h>

typedef struct {
  // defines the maximum amount of memory purple garden is allowed to allocate,
  // if this is hit, the vm exits with a non zero code
  uint64_t max_memory;
  // removes all default builtins like @len, @type, etc
  bool remove_default_builtins;
  // disables the @std/ namespace
  bool disable_std_namespace;
  // disables garbage collection and allocates 'max_memory' with a bump
  // allocator
  bool disable_gc;
} Vm_Config;

#define ENCODE_ARG_COUNT_AND_OFFSET(COUNT, OFFSET)                             \
  /* offset-1 since r0 is always allocated */                                  \
  (((COUNT) & 0x7F) << 7 | ((OFFSET - 1) & 0x7F))
#define DECODE_ARG_COUNT(ARG) (((ARG) >> 7) & 0x7F)
#define DECODE_ARG_OFFSET(ARG) ((ARG) & 0x7F)

typedef enum {
  // create array in vm
  VM_NEW_ARRAY,
  // create obj in vm
  VM_NEW_OBJ,
} VM_New;

// PERF: maybe _ is too many, but prefetching a recursion depth can have
// some positive effects on the runtime performance
#define PREALLOCATE_FREELIST_SIZE 0

// A frame represents a Scope, a new scope is created upon entering a function -
// since functions are pure, there is no way to interact with the previous frame
// inside of a function, the pointer is kept to allow the runtime to restore the
// scope state to its state before entering the functions scope
typedef struct Frame {
  struct Frame *prev;
  // returning out of scope, we need to jump back to the callsite of the
  // function
  size_t return_to_bytecode;
  // stores Values by their hash, serving as a variable table
  Map variable_table; // TODO: use a different Map impl for this; since we keep
                      // 256 variables in the current scope at max; we compute
                      // all caps at compile time and we dont need any collision
                      // checking at this point
} Frame;

typedef struct __Vm {
  uint32_t global_len;
  // globals represents the global pool created by the bytecode compiler
  Value *globals;

  uint64_t bytecode_len;
  // TODO: replace this with a 32bit wide instruction and encode op, arg1, arg2
  // in it
  uint32_t *bytecode;

  // current position in the bytecode
  size_t pc;
  Value registers[REGISTERS + 1];

  // frame stores variables of the current scope, meta data and other required
  // data
  Frame *frame;

  // encode amount of arguments to function call, can be CALL or BUILTIN
  uint16_t arg_count;
  // encode offset to know where the arguments start in the register block
  uint16_t arg_offset;

  // used for container sizes and stuff
  uint32_t size_hint;

  // i have to type erase here :(
  void **builtins;

  Allocator *alloc;
#if DEBUG
  uint64_t instruction_counter[256];
#endif
} Vm;

// Represents the type signature for a builtin function
typedef void (*builtin_function)(Vm *vm);

#define GLOBAL_FALSE 0
#define GLOBAL_TRUE 1
#define GLOBAL_NONE 2

typedef enum {
  // STORE rANY
  //
  // STORE a Value from r0 into an arbitrary register
  OP_STORE,

  // LOAD rANY
  //
  // LOAD a Value from rANY into r0
  OP_LOAD,

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

  // OP_EQ rANY
  //
  // compares value at r0 and rANY via Value_cmp
  OP_EQ = 6,

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
  // Ends a scope
  OP_LEAVE,

  // OP_CALL ADDR
  //
  // 1: enters a new stackframe, stores the last vm->pc in
  // Frame.return_to_bytecode
  //
  // 2: jumps to ADDR
  OP_CALL,

  // OP_JMP bc
  //
  // Jumps to bc in bytecode index, does no bounds checking
  OP_JMP,

  // OP_ASSERT
  //
  // stops execution with error message if r0 evals to false
  OP_ASSERT,

  // LOADG rANY
  //
  // LOADG a global from the const table to r0
  OP_LOADG,

  // JMPF bANY
  //
  // jump to bANY if r0 is false
  OP_JMPF,

  // NEW oType
  //
  // Creates an instance of the value defined via its argument, see enum
  // VM_New
  OP_NEW,

  // APPEND rANY
  //
  // Appends r0 to rANY
  OP_APPEND,

  // Size uint32_t
  //
  // Specifices a size
  OP_SIZE,
} VM_OP;

#define VM_ERR(fmt, ...)                                                       \
  fprintf(stderr, "[VM] ERROR: " fmt "\n", ##__VA_ARGS__);                     \
  goto vm_end;

extern Str OP_MAP[];

// Creates a new virtual machine with registered builtins, static_alloc is used
// to allocate space for both the global pool and the byte code space, alloc is
// used in the virtual machine itself
Vm Vm_new(Vm_Config conf, Allocator *static_alloc, Allocator *alloc);
void Vm_register_builtin(Vm *vm, builtin_function bf, Str name);
int Vm_run(Vm *vm);
void Vm_destroy(Vm *vm);
