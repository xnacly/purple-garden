#ifndef BUILTINS_H
#define BUILTINS_H

#include "common.h"

typedef enum {
  BUILTIN_UNKOWN,
  BUILTIN_PRINTLN,
  BUILTIN_PRINT,
  BUILTIN_TYPE,
  BUILTIN_LEN,
} Builtin;

// each builtin must be defined in this header file both inside the enum below
// (`BUILTIN_<name>`) and as a signature
// `Value builtin_<name>(const Value args)`, it also needs to be added to
// `BUILTIN_MAP` via its enum value
// `{[BUILTIN_<name>] = &builtin_<name> }`. The argument Value is constant,
// because all builtins are pure.
typedef Value (*builtin_function)(const Value *args, size_t count);
extern builtin_function BUILTIN_MAP[];
extern Str BUILTIN_NAME_MAP[];
#endif
