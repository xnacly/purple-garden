#include "builder.h"
#include <string.h>

#define BUILDER_DEFAULT_SIZE 256

Builder Builder_new(Allocator *alloc, size_t size) {
  size_t cap = size ? size : BUILDER_DEFAULT_SIZE;
  return (Builder){.alloc = alloc,
                   .cap = cap,
                   .buffer = alloc->request(alloc->ctx, cap * sizeof(uint8_t)),
                   .len = 0};
}

static inline void Builder_grow(Builder *builder) {
  // *1.5x growth
  builder->cap += (builder->cap >> 1);
  size_t new_size = builder->cap * sizeof(uint8_t);
  uint8_t *old = builder->buffer;
  builder->buffer = builder->alloc->request(builder->alloc->ctx, new_size);
  memcpy(builder->buffer, old, new_size);
}

void Builder_append_Str(Builder *builder, Str s) {
  if (builder->len + s.len > builder->cap)
    Builder_grow(builder);
  memcpy(builder->buffer + builder->len, s.p, s.len);
  builder->len += s.len;
}

void Builder_append_char(Builder *builder, char c) {
  if (builder->len + 1 > builder->cap) {
    Builder_grow(builder);
  }
  builder->buffer[builder->len++] = (uint8_t)c;
}

void Builder_append_byte(Builder *builder, uint8_t b) {
  if (builder->len + 1 > builder->cap) {
    Builder_grow(builder);
  }
  builder->buffer[builder->len++] = b;
}

Str Builder_as_Str(const Builder *builder) {
  return (Str){.len = builder->len, .p = builder->buffer};
}

const char *Builder_as_cstr(const Builder *builder) {
  char *cstr = builder->alloc->request(builder->alloc->ctx, builder->len + 1);
  memcpy(cstr, builder->buffer, builder->len);
  cstr[builder->len] = '\0';
  return cstr;
}
