#pragma once

#include "mem.h"
#include <stddef.h>
#include <stdint.h>

// TODO: add a tag for Str.p not pointing into an mmaped space => they need to
// be deallocated if dangling

// str is a simple stack allocated wrapper around C char arrays, providing
// constant time length access and zero allocation+copy interactions for all
// methods except if Allocator is present as its argument. Cheap to copy.
typedef struct __Str {
  // hash of the input, do not expect it to be filled, has to be computed via
  // Str_hash or inline in the lexer
  uint64_t hash;
  // length of the input without a zero terminator
  uint32_t len;
  // store the pointer to the underlying char
  const uint8_t *p;
} Str;

#define FNV_OFFSET_BASIS 0x811c9dc5
#define FNV_PRIME 0x01000193

#define STRING(str) ((Str){.len = sizeof(str) - 1, .p = (const uint8_t *)str})
#define STRING_EMPTY ((Str){.len = 0, .p = NULL})

char Str_get(const Str *str, size_t index);
Str Str_from(const char *s);
Str Str_slice(const Str *str, size_t start, size_t end);
Str Str_concat(const Str *a, const Str *b, Allocator *alloc);
bool Str_eq(const Str *a, const Str *b);
void Str_debug(const Str *str);
uint32_t Str_hash(const Str *str);
int64_t Str_to_int64_t(const Str *str);
double Str_to_double(const Str *str);
