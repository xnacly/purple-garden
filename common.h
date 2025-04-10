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
#define ASSERT(EXP, context)                                                   \
  if (!(UNLIKELY(EXP))) {                                                      \
    fprintf(stderr,                                                            \
            "purple-garden: ASSERT(" #EXP "): `" context                       \
            "` failed at %s, line %d\n",                                       \
            __FILE__, __LINE__);                                               \
    exit(EXIT_FAILURE);                                                        \
  }

#define TODO(msg) ASSERT(0, "TODO: " msg)

#include "strings.h"

typedef enum {
  V_UNDEFINED,
  V_OPTION,
  V_STRING,
  V_NUM,
  V_TRUE,
  V_FALSE,
  V_LIST,
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
    struct Option {
      bool is_some;
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
