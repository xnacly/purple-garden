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

#define BYTECODE_SIZE (5 * 1024 * 1024)
#define GLOBAL_SIZE 512 * 1024
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
  V_NONE,
  V_STR,
  V_DOUBLE,
  V_INT,
  V_TRUE,
  V_FALSE,
  V_ARRAY,
} ValueType;

extern Str VALUE_TYPE_MAP[];

// List is purple gardens internal array representation. It is implemented as a
// growing array and can be configured via the LIST_* macros. It owns its
// values. Can be used like so:
//
//     #include "adts.h"
//
//     List l = List_new(8, vm->alloc);
//     List_append(&l, *INTERNED_TRUE);
//     List_append(&l, *INTERNED_NONE);
//     List_append(
//         &l, (Value){.type = V_STR, .is_some = true, .string =
//         STRING("HOLA")});
//     Value array = (Value){.type = V_ARRAY, .array = l};
typedef struct {
  size_t cap;
  size_t len;
  struct Value *elements; // voided because c sucks with selfreferencing types
  Allocator *a;
} List;

// Map is purple gardens internal hash map representation. It is implemented as
// a list of buckets, in which each bucket is a List, thus enabling hash
// collision resolving. Can be configured via the MAP_* macros. It owns its
// values.
typedef struct {
  size_t size;
  List *buckets;
  Allocator *a;
} Map;

// Value represents a value known to the runtime
typedef struct {
  ValueType type;
  // true if @Some, otherwise self is just a Value, not an option with said
  // inner Value
  bool is_some;
  // Value can also be just an option, similar to Rusts option if type is
  // V_OPTION and .is_some is false, this acts as a NONE value
  union {
    Str string;
    double floating;
    int64_t integer;
    List array;
    Map obj;
  };
} Value;

// global values that compiler, optimiser and vm use, often mapped to global
// pool indexes 0,1,2
static Value *INTERNED_FALSE = &(Value){.type = V_FALSE};
static Value *INTERNED_TRUE = &(Value){.type = V_TRUE};
static Value *INTERNED_NONE = &(Value){.type = V_NONE};

bool Value_cmp(const Value *a, const Value *b);
void Value_debug(const Value *v);
double Value_as_double(const Value *v);
