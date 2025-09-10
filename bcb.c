#include "cc.h"
#include "common.h"
#include "mem.h"

ByteCodeBuilder ByteCodeBuilder_new(Allocator *a) {
  size_t cap = INIT_BYTECODE_SIZE;
  return (ByteCodeBuilder){
      .alloc = a,
      .cap = cap,
      .buffer = (uint32_t *)CALL(a, request, cap * sizeof(uint32_t)),
      .len = 0,
  };
}

#define GROWTH 2

static void grow(ByteCodeBuilder *bcb, Allocator *a) {
  size_t new_cap = bcb->cap * GROWTH;
  uint32_t *old = bcb->buffer;
  bcb->buffer = CALL(a, request, sizeof(uint32_t) * new_cap);
  memcpy(bcb->buffer, old, sizeof(uint32_t) * bcb->len);
  bcb->cap = new_cap;
}

void ByteCodeBuilder_add(ByteCodeBuilder *bcb, uint32_t op, uint32_t arg) {
  if (bcb->len + 2 > bcb->cap) {
    grow(bcb, bcb->alloc);
  }

  bcb->buffer[bcb->len++] = op;
  bcb->buffer[bcb->len++] = arg;
}

void ByteCodeBuilder_insert_arg(ByteCodeBuilder *bcb, size_t idx,
                                uint32_t arg) {
  ASSERT(idx + 1 < bcb->len, "Can't insert out of allocation bounds");
  bcb->buffer[idx + 1] = arg;
}
