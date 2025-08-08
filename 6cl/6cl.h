/*
 * copied from https://github.com/xNaCly/6cl since I dont care about git
 * submodules, maybe in the future
 * */
#pragma once
#include <stddef.h>
#include <stdbool.h>

#ifndef SIX_OPTION_PREFIX
#define SIX_OPTION_PREFIX '+'
#endif

#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif

#ifndef SIX_MAX_REST
#define SIX_MAX_REST 16
#endif

typedef enum {
  SIX_STR,
  SIX_BOOL,
  SIX_CHAR,
  SIX_INT,
  SIX_LONG,
  SIX_FLOAT,
  SIX_DOUBLE,
} SixFlagType;

// SixClFlag defines a singular command line option and will hold the value once
// parsing is done
typedef struct {
  // name of the flag, for instance +<name>; +help
  const char *name;
  // short name, like +<short_name>; +h
  char short_name;
  // Defines the datatype
  SixFlagType type;
  // used in the help page
  const char *description;

  // typed result values, will be filled with the value if any is found found
  // for the option, or with the default value thats already set.
  union {
    // string value
    const char *s;
    // boolean value
    bool b;
    // char value
    char c;
    // int value
    int i;
    // long value
    long l;
    // float value
    float f;
    // double value
    double d;
  };
} SixFlag;

typedef struct Six {
  SixFlag *flags;
  size_t flag_count;
  // usage will be postfixed with this
  const char *name_for_rest_arguments;
  // rest holds all arguments not matching any defined options
  char *rest[SIX_MAX_REST];
  size_t rest_count;
} Six;

void SixParse(Six *six, size_t argc, char **argv);
