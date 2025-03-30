#ifndef COMMON_H
#define COMMON_H

#include <stdint.h>

#ifndef DEBUG
#define DEBUG 0
#endif

#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>

#define ASSERT(EXP, context)                                                   \
  if (!(EXP)) {                                                                \
    fprintf(stderr,                                                            \
            "purple-garden: ASSERT(" #EXP "): `" context                       \
            "` failed at %s, line %d\n",                                       \
            __FILE__, __LINE__);                                               \
    exit(EXIT_FAILURE);                                                        \
  }

#define TODO(msg) ASSERT(0, "TODO: " msg)

// String is a simple wrapper around C char arrays, providing constant time
// length access and zero allocation+copy interactions for all methods except
// String_to
typedef struct {
  // store the pointer to the underlying char
  char *p;
  // length of the input without a zero terminator
  size_t len;
} String;

#define STRING(str) ((String){.len = sizeof(str) - 1, .p = str})
#define STRING_EMPTY ((String){.len = 0, .p = NULL})

// String_get enables accessing a character at a position of a string with
// bounds checking
char String_get(String *str, size_t index);

// String_to converts str to a c like string (causes allocation, requires
// deallocation by caller)
char *String_to(String *str);

// String_from converts s to a String
String String_from(char *s);

// String_slice returns a slice of str from start to end
String String_slice(String *str, size_t start, size_t end);

// String_eq returns true if a and b are equal
bool String_eq(String *a, String *b);

// String_debug prints the content of str to stdout
void String_debug(String *str);

#endif
