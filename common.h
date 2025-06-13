#pragma once

#include "mem.h"
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#ifndef DEBUG
#define DEBUG 0
#endif

#ifndef MIN_MEM
#define MIN_MEM 4 * 1024 * 1024
#endif

#define BYTECODE_SIZE (4 * 1024 * 1024)
#define GLOBAL_SIZE 4 * 1024 * 1024
#define MAX_BUILTIN_SIZE 1024
#define MAX_BUILTIN_SIZE_MASK (MAX_BUILTIN_SIZE - 1)

#define REGISTERS 127
#define CALL_ARGUMENT_STACK 256
#define VARIABLE_TABLE_SIZE 256
#define VARIABLE_TABLE_SIZE_MASK (VARIABLE_TABLE_SIZE - 1)

#define UNLIKELY(condition) __builtin_expect(condition, 0)
#define ASSERT(EXP, fmt, ...)                                                  \
  if (!(UNLIKELY(EXP))) {                                                      \
    fprintf(stderr,                                                            \
            "purple-garden: ASSERT(" #EXP "): " fmt                            \
            " failed at %s, line %d\n",                                        \
            ##__VA_ARGS__, __FILE__, __LINE__);                                \
    exit(EXIT_FAILURE);                                                        \
  }

#define TODO(fmt, ...)                                                         \
  fprintf(stderr, "TODO: " fmt " failed in " __FILE__ ":%d\n", ##__VA_ARGS__,  \
          __LINE__);                                                           \
  exit(EXIT_FAILURE);

#include "strings.h"

typedef enum {
  V_UNDEFINED,
  V_OPTION,
  V_STR,
  V_DOUBLE,
  V_INT,
  V_TRUE,
  V_FALSE,
  V_ARRAY,
} ValueType;

extern Str VALUE_TYPE_MAP[];

// Value represents a value known to the runtime
typedef struct Value {
  ValueType type;
  // Value can also be just an option, similar to Rusts option if type is
  // V_OPTION and .is_some is false, this acts as a NONE value
  union {
    Str string;
    double floating;
    int64_t integer;
    struct Array {
      size_t len;
      // holds members of the array
      struct Value **value;
    } array;

    struct Option {
      bool is_some;
      // holds some
      const struct Value *value;
    } option;
  };
} Value;

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
  Value *variable_table[VARIABLE_TABLE_SIZE];
} Frame;

typedef struct {
  uint32_t global_len;
  // globals represents the global pool created by the bytecode compiler
  Value **globals;

  uint64_t bytecode_len;
  uint32_t *bytecode;

  // current position in the bytecode
  size_t pc;
  Value registers[REGISTERS + 1];

  // frame stores variables of the current scope, meta data and other required
  // data
  Frame *frame;

  // arg_count enables the vm to know how many register it needs to read
  // and pass to the function called via CALL or BUILTIN
  size_t arg_count;

  // i have to type erase here :(
  void **builtins;

  Allocator *alloc;
#if DEBUG
  size_t instruction_counter[256];
#endif
} Vm;

// Represents the type signature for a builtin function
typedef void (*builtin_function)(Vm *vm);

bool Value_cmp(const Value *a, const Value *b);
void Value_debug(const Value *v);
double Value_as_double(const Value *v);

#define SOME(val)                                                              \
  (Option) { .is_some = true, .some = val }
