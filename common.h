#ifndef COMMON_H
#define COMMON_H

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
  V_NUM,
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
    double number;
    struct {
      size_t len;
      // holds members of the array
      struct Value *value;
    } array;
    struct Option {
      bool is_some;
      // holds some
      struct Value *value;
    } option;
  };
} Value;

#define SOME(val)                                                              \
  (Option) { .is_some = true, .some = val }

#define NONE                                                                   \
  (Value) {                                                                    \
    .type = V_OPTION, .option = (struct Option) { .is_some = false }           \
  }

bool Value_cmp(Value a, Value b);

#endif
