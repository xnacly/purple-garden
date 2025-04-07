#ifndef BUILTINS_H
#define BUILTINS_H

#include "common.h"

typedef enum {
  BUILTIN_UNKOWN,
  BUILTIN_PRINTLN,
  BUILTIN_PRINT,
  BUILTIN_LEN,
} Builtin;

// each builtin must be defined in this header file both inside the enum below
// (`BUILTIN_<name>`) and as a signature `Value builtin_<name>(const Value
// args)`, it also needs to be added to BUILTIN_MAP via its enum value
// `{[BUILTIN_<name>] = &builtin_<name> }`. The argument Value is constant,
// because all builtins are pure.
typedef Value (*builtin_function)(const Value args);
extern builtin_function BUILTIN_MAP[];

extern Str BUILTIN_NAME_MAP[];

// println outputs its argument to stdout, suffixed with a newline
Value builtin_println(const Value args);

// print outputs its argument to stdout
Value builtin_print(const Value args);

// len returns the value of its argument:
//
// - for V_STRING: string length
// - for V_LIST: amount of children in list
// - else 0
Value builtin_len(const Value args);

#endif
