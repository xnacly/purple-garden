#ifndef COMMON_H
#define COMMON_H

#include <stddef.h>
#include <stdio.h>

#define ASSERT(EXP, context)                                                   \
  if (!(EXP)) {                                                                \
    fprintf(stderr,                                                            \
            "purple-garden: ASSERT(" #EXP "): `" context                       \
            "` failed at %s, line %d\n",                                       \
            __FILE__, __LINE__);                                               \
    exit(EXIT_FAILURE);                                                        \
  }

typedef short boolean;
#define true (1)
#define false (0)

// String is a simple wrapper around C char arrays, providing constant time
// length access
typedef struct {
  // store the pointer to the underlying char
  char *p;
  // length of the input - zero terminator
  size_t len;
} String;

#define STRING(str) ((String){.len = sizeof(str), .p = str})
#define STRING_EMPTY ((String){.len = 0, .p = NULL})

// String_get enables accessing a character at a position of a string with
// bounds checking
char String_get(String *str, size_t index);

// String_to converts str to a c like string
char *String_to(String *str);

// String_from converts s to a String
String String_from(char *s);

// String_slice returns a slice of str from start to end (causes allocation)
String String_slice(String *str, size_t start, size_t end);

// String_eq returns true if a and b are equal
boolean String_eq(String *a, String *b);

#endif
