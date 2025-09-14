#pragma once

#include "adts.h"
#include "mem.h"
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#ifndef DEBUG
#define DEBUG 0
#endif

#define INIT_BYTECODE_SIZE 256
#define GLOBAL_SIZE 512
#define GLOBAL_SIZE_MASK (GLOBAL_SIZE - 1)
#define MAX_BUILTIN_SIZE 1024
#define MAX_BUILTIN_SIZE_MASK (MAX_BUILTIN_SIZE - 1)

#ifndef MIN_MEM
#define MIN_MEM                                                                \
  INIT_BYTECODE_SIZE * sizeof(uint32_t) + 2 * GLOBAL_SIZE * sizeof(Value) +    \
      MAX_BUILTIN_SIZE * sizeof(builtin_function)
#endif

#define REGISTERS 31
#define CALL_ARGUMENT_STACK 256
#define VARIABLE_TABLE_SIZE 256
#define VARIABLE_TABLE_SIZE_MASK (VARIABLE_TABLE_SIZE - 1)

#define DBG(EXPR)                                                              \
  ({                                                                           \
    _Pragma("GCC diagnostic push")                                             \
        _Pragma("GCC diagnostic ignored \"-Wformat\"") __auto_type _val =      \
            (EXPR);                                                            \
    fprintf(stderr, "[%s:%d] %s = ", __FILE__, __LINE__, #EXPR);               \
    _Generic(_val,                                                             \
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
        struct ListIdx: fprintf(stderr,                                        \
                                "ListIdx {.block=%u, .block_idx=%u}\n", _val), \
        default: fprintf(stderr, "<unprintable>\n"));                          \
    _Pragma("GCC diagnostic pop") _val;                                        \
  })

#define UNLIKELY(condition) __builtin_expect(condition, 0)
// not compiled out in release builds
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

LIST_TYPE(Value);

// TODO: make this smaller, its really is necessary (32 bytes and 8 byte padding
// IS WAY too large); see NaN-Boxing

// Value represents a value known to the runtime
typedef struct Value {
  // true if @Some, otherwise self is just a Value, if @None just .type=V_NONE
  unsigned int is_some : 1;
  unsigned int type : 3;
  union {
    const Str *string;
    LIST_Value *array;
    Map *obj;
    double floating;
    int64_t integer;
  };
} Value;

typedef struct Map {
  LIST_Value entries;
  uint64_t cap;
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
