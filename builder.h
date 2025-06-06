#pragma once

#include "common.h"
#include "mem.h"
#include "strings.h"

// Builder functions as a growing list of bytes that accepts a string, a char
// and a byte as appendees, while providing the ability to export the contents
// either as a cstr (needs an allocation), or a Str abstraction
typedef struct {
  size_t len;
  size_t cap;
  Allocator *alloc;
  uint8_t *buffer;
} Builder;

Builder Builder_new(Allocator *alloc, size_t size);
void Builder_append_Str(Builder *builder, Str s);
void Builder_append_char(Builder *builder, char c);
void Builder_append_byte(Builder *builder, uint8_t b);
Str Builder_as_str(const Builder *builder);
const char *Builder_to_cstr(const Builder *builder);
