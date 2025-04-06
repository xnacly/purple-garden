#ifndef STRINGS_H
#define STRINGS_H

#include <stddef.h>

// str is a simple stack allocated wrapper around C char arrays, providing
// constant time length access and zero allocation+copy interactions for all
// methods except Str_to
typedef struct {
  // store the pointer to the underlying char
  const char *p;
  // length of the input without a zero terminator
  size_t len;
} Str;

#define STRING(str) ((Str){.len = sizeof(str) - 1, .p = str})
#define STRING_EMPTY ((Str){.len = 0, .p = NULL})

// Str_get enables accessing a character at a position of a string with
// bounds checking
char Str_get(const Str *str, size_t index);

// Str_to converts str to a c like string (causes allocation, requires
// deallocation by caller)
char *Str_to(const Str *str);

// Str_from converts s to a Str
Str Str_from(const char *s);

// Str_slice returns a slice of str from start to end
Str Str_slice(const Str *str, size_t start, size_t end);

// Str_eq returns true if a and b are equal
bool Str_eq(const Str *a, const Str *b);

// Str_debug prints the content of str to stdout
void Str_debug(const Str *str);

// Str_hash computes a hash str
size_t Str_hash(const Str *str);

#endif
