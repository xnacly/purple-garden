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
  byte *buffer;
} Builder;

Builder Builder_new(Allocator *alloc, size_t size);
void Builder_append_str(Builder *builder, Str s);
void Builder_append_char(Builder *builder, char c);
void Builder_append_byte(Builder *builder, byte b);
Str Builder_to_str(Builder *builder);
char *Builder_to_cstr(Builder *builder);
