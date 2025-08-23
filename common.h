#pragma once

#include "mem.h"
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#ifndef DEBUG
#define DEBUG 0
#endif

#define BYTECODE_SIZE (2 * 1024 * 1024)
#define GLOBAL_SIZE 512
#define GLOBAL_SIZE_MASK (GLOBAL_SIZE - 1)
#define MAX_BUILTIN_SIZE 1024
#define MAX_BUILTIN_SIZE_MASK (MAX_BUILTIN_SIZE - 1)

#ifndef MIN_MEM
#define MIN_MEM                                                                \
  BYTECODE_SIZE * sizeof(uint32_t) + 2 * GLOBAL_SIZE * sizeof(Value) +         \
      MAX_BUILTIN_SIZE * sizeof(builtin_function)
#endif

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

extern Str VALUE_TYPE_MAP[];

typedef struct Value Value; // forward declared so the compiler knows a thing or
                            // two about a thing or two

typedef enum {
  V_NONE,
  V_STR,
  V_DOUBLE,
  V_INT,
  V_TRUE,
  V_FALSE,
  V_ARRAY,
  V_OBJ,
} ValueType;

// List is purple gardens internal array representation. It is implemented as a
// growing array and can be configured via the LIST_* macros. It owns its
// values. Can be used like so:
//
//     #include "adts.h"
//
//     List l = List_new(8, vm->alloc);
//     List_append(&l, *INTERNED_TRUE, vm->alloc);
//     List_append(&l, *INTERNED_NONE, vm->alloc);
//     List_append(
//         &l, (Value){.type = V_STR, .is_some = true, .string =
//         STRING("HOLA")},
//         vm->alloc
//     );
//     Value array = (Value){.type = V_ARRAY, .array = l};
//
// List will be based on zigs segmented list and has the advantage of not
// needing to copy its previous members on growing
typedef struct {
  uint32_t cap;
  uint32_t len;
  Value *elements;
  // TODO:
  // https://github.com/ziglang/zig/blob/e17a050bc695f7d117b89adb1d258813593ca111/lib/std/segmented_list.zig
  // and https://danielchasehooper.com/posts/segment_array/
} List;

// Map is purple gardens internal hash map representation. It is implemented as
// a list of buckets, in which each bucket is a List, thus enabling hash
// collision resolving. Can be configured via the MAP_* macros. It owns its
// values.
typedef struct {
  size_t size;
  List *buckets;
} Map;

// Value represents a value known to the runtime
typedef struct Value {
  // true if @Some, otherwise self is just a Value, if @None just .type=V_NONE
  unsigned int is_some : 1;
  unsigned int type : 3;
  union {
    Str string;
    List array;
    Map obj;
    double floating;
    int64_t integer;
  };
} Value;

// global values that compiler, optimiser and vm use, often mapped to global
// pool indexes 0,1,2
__attribute__((unused)) static Value *INTERNED_FALSE =
    &(Value){.type = V_FALSE};
__attribute__((unused)) static Value *INTERNED_TRUE = &(Value){.type = V_TRUE};
__attribute__((unused)) static Value *INTERNED_NONE = &(Value){.type = V_NONE};

bool Value_cmp(const Value *a, const Value *b);
void Value_debug(const Value *v);
double Value_as_double(const Value *v);
