#ifndef BUILTINS_H
#define BUILTINS_H

#include "common.h"

enum {
  BUILTIN_PRINTLN,
  BUILTIN_PRINT,
  BUILTIN_LEN,
};

// each builtin must be defined in this header file both inside the enum below
// (`BUILTIN_<name>`) and as a signature `Value builtin_<name>(Value *args)`,
// it also needs to be added to BUILTIN_MAP via its enum value
// `{[BUILTIN_<name>] = &builtin_<name> }`
typedef Value (*builtin_function)(Value *args);
extern builtin_function BUILTIN_MAP[];

// println outputs its argument to stdout, suffixed with a newline
Value builtin_println(Value *args);

// print outputs its argument to stdout
Value builtin_print(Value *args);

// len returns the value of its argument:
//
// - for V_STRING: string length
// - for V_LIST: amount of children in list
// - else 0
Value builtin_len(Value *args);

#endif
