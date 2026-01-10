#pragma once

#include "adts.h"
#include "mem.h"
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#define ARG(I) (vm->registers[vm->arg_offset + 1 + (I)])
#define RETURN(...) (vm->registers[0] = (Value)__VA_ARGS__)

#ifndef DEBUG
#define DEBUG 0
#endif

#define INIT_BYTECODE_SIZE 64
#define GLOBAL_SIZE 1024
#define GLOBAL_SIZE_MASK (GLOBAL_SIZE - 1)
#define MAX_BUILTIN_SIZE 1024
#define MAX_BUILTIN_SIZE_MASK (MAX_BUILTIN_SIZE - 1)

#ifndef MIN_MEM
#define MIN_MEM                                                                \
  INIT_BYTECODE_SIZE * sizeof(uint32_t) + 2 * GLOBAL_SIZE * sizeof(Value) +    \
      MAX_BUILTIN_SIZE * sizeof(builtin_function)
#endif

#define REGISTERS 16

#define SWAP_STRUCT(A, B)                                                      \
  do {                                                                         \
    _Static_assert(__builtin_types_compatible_p(typeof(A), typeof(B)),         \
                   "SWAP_STRUCT arguments must have identical types");         \
                                                                               \
    typeof(A) __swap_tmp = (A);                                                \
    (A) = (B);                                                                 \
    (B) = __swap_tmp;                                                          \
  } while (0)

#define UNLIKELY(condition) __builtin_expect(condition, 0)
// TODO: not compiled out in release builds; rework this into a panic system and
// compile asserts out for release
#define ASSERT(EXP, fmt, ...)                                                  \
  if (!(UNLIKELY(EXP))) {                                                      \
    fprintf(stderr,                                                            \
            "purple-garden: ASSERT(" #EXP "): " fmt                            \
            " failed at %s, line %d\n",                                        \
            ##__VA_ARGS__, __FILE__, __LINE__);                                \
    exit(EXIT_FAILURE);                                                        \
  }

#define TODO(MSG)                                                              \
  fprintf(stderr, "TODO: %s in %s %s:%d\n", MSG, __func__, __FILE__,           \
          __LINE__);                                                           \
  exit(EXIT_FAILURE);

#define MAX(a, b) ((a) > (b) ? (a) : (b))

#include "strings.h"

extern Str VALUE_TYPE_MAP[];

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

// dynamically growing array
typedef struct List {
  size_t cap;
  size_t len;
  Value *arr;
} List;

// Value represents a value known to the runtime
typedef struct Value {
  // true if Some, otherwise self is just a Value, if None just .type=V_NONE
  unsigned int is_some : 1;
  // TODO: move this to Str.is_heap, since array, obj are always gc heap
  // allocated, but Str can point to compile time known strings, only upon
  // creating new strings in the runtime, String views are gc heap allocated,
  // their inner buffer is too
  unsigned int is_heap : 1;
  unsigned int type : 3;
  union {
    Str string;
    List *array;
    Map *obj;
    double floating;
    int64_t integer;
  };
} Value;

#define V_NUM_MASK ((1 << V_INT) | (1 << V_DOUBLE))

typedef struct MapEntry {
  uint32_t hash;
  Value value;
} MapEntry;

typedef struct Map {
  size_t cap;
  size_t len;
  MapEntry *buckets;
} Map;

// global values that compiler, optimiser and vm use, often mapped to global
// pool indexes 0,1,2
__attribute__((unused)) static const Value *INTERNED_FALSE =
    &(Value){.type = V_FALSE};
__attribute__((unused)) static const Value *INTERNED_TRUE =
    &(Value){.type = V_TRUE};
__attribute__((unused)) static const Value *INTERNED_NONE =
    &(Value){.type = V_NONE};

bool Value_cmp(const Value *a, const Value *b);
void Value_debug(const Value *v);
double Value_as_double(const Value *v);
int64_t Value_as_int(const Value *v);
bool Value_is_opt(const Value *v);

#define DBG(EXPR)                                                              \
  ({                                                                           \
    _Pragma("GCC diagnostic push")                                             \
        _Pragma("GCC diagnostic ignored \"-Wformat\"") __auto_type _val =      \
            (EXPR);                                                            \
    fprintf(stderr, "[%s:%d] %s = ", __FILE__, __LINE__, #EXPR);               \
    _Generic((_val),                                                           \
        int: fprintf(stderr, "%d\n", _val),                                    \
        long: fprintf(stderr, "%ld\n", _val),                                  \
        long long: fprintf(stderr, "%lld\n", _val),                            \
        unsigned: fprintf(stderr, "%u\n", _val),                               \
        unsigned long: fprintf(stderr, "%lu\n", _val),                         \
        unsigned long long: fprintf(stderr, "%llu\n", _val),                   \
        float: fprintf(stderr, "%f\n", _val),                                  \
        double: fprintf(stderr, "%f\n", _val),                                 \
        const char *: fprintf(stderr, "\"%s\"\n", _val),                       \
        char *: fprintf(stderr, "\"%s\"\n", _val),                             \
        default: fprintf(stderr, "<unprintable>\n"));                          \
    _Pragma("GCC diagnostic pop") _val;                                        \
  })
