#include "adts.h"
#include "cc.h"
#include "mem.h"
#include <stdint.h>
#include <string.h>

ByteCodeBuilder ByteCodeBuilder_new(Allocator *a) {
  ByteCodeBuilder bcb = {.alloc = a, .buffer = LIST_new(uint32_t)};

  // we preallocate here so middle large scripts dont require many newblock
  // operations, S of n blocks is: LIST_DEFAULT_SIZE * (2^i - 1); for 8 thats
  // around 2040 fields or 1020 instruction and argument pairs
#define PREALLOCATE_BLOCKS_IN_BUFFER 8
  bcb.buffer.blocks = CALL(a, request, LIST_BLOCK_COUNT * sizeof(uint32_t *));
  for (size_t i = 0; i < PREALLOCATE_BLOCKS_IN_BUFFER; i++) {
    bcb.buffer.blocks[i] =
        CALL(a, request, (LIST_DEFAULT_SIZE << i) * sizeof(uint32_t));
  };

  return bcb;
}

inline void ByteCodeBuilder_add(ByteCodeBuilder *bcb, uint32_t op,
                                uint32_t arg) {
  LIST_append(&bcb->buffer, bcb->alloc, op);
  LIST_append(&bcb->buffer, bcb->alloc, arg);
}

inline void ByteCodeBuilder_insert_arg(ByteCodeBuilder *bcb, size_t idx,
                                       uint32_t arg) {
  LIST_insert_UNSAFE(&bcb->buffer, idx + 1, arg);
}

uint32_t *ByteCodeBuilder_to_buffer(const ByteCodeBuilder *bcb) {
  size_t size = bcb->buffer.len * sizeof(uint32_t);
  LIST_uint32_t l = bcb->buffer;
  if (l.blocks == NULL || l.len == 0)
    return NULL;

  uint32_t *flat = CALL(bcb->alloc, request, size);
  size_t offset = 0;

  // same as LIST_BLOCK_COUNT, but the pragma somehow doesnt want my constant
#pragma GCC unroll 24
  for (size_t block = 0; block < LIST_BLOCK_COUNT && offset < l.len; block++) {
    if (l.blocks[block] == NULL)
      break;

    size_t block_size = LIST_DEFAULT_SIZE << block;
    size_t remaining = l.len - offset;
    size_t to_copy = remaining < block_size ? remaining : block_size;

    memcpy(flat + offset, l.blocks[block], to_copy * sizeof(uint32_t));
    offset += to_copy;
  }

  return flat;
}
