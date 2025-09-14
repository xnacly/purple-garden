#include "adts.h"

#include <string.h>

struct ListIdx idx_to_block_idx(size_t idx) {
  struct ListIdx r = {0};
  if (idx < LIST_DEFAULT_SIZE) {
    r.block_idx = idx;
    return r;
  }

  // This optimizes the block index lookup to be constant time
  //
  //     block 0 size = LIST_DEFAULT_SIZE
  //     block 1 size = LIST_DEFAULT_SIZE*2
  //     block 2 size = LIST_DEFAULT_SIZE*4
  //     block 3 size = LIST_DEFAULT_SIZE*8
  //
  // The starting index of each block is a geometric series:
  //
  //    s(i) = LIST_DEFAULT_SIZE * (2^i - 1)
  //
  // We solve for i, so the following stands:
  //
  //    s(i) <= idx < s(i+1)
  //
  //    2^i - 1 <= idx / LIST_DEFAULT_SIZE < 2^(i+1) - 1
  //    idx / LIST_DEFAULT_SIZE + 1 >= 2^i
  //
  // Thus: adding LIST_DEFAULT_SIZE to idx shifts the series so the msb of idx +
  // LIST_DEFAULT_SIZE correspond to the block number
  //
  // Visually:
  //
  //     Global index:  0 1 2 3 4 5 6 7  |  8  9 10 ... 23  | 24 25 ... 55  | 56
  //     ... Block:         0                 |  1              |  2 | 3 ...
  //     Block size:    8                 | 16              | 32            | 64
  //     ... idx + LIST_DEFAULT_SIZE: 0+8=8  -> MSB pos 3 -> block 0 7+8=15 ->
  //     MSB pos 3 -> block 0 8+8=16 -> MSB pos 4 -> block 1 23+8=31-> MSB pos 4
  //     -> block 1 24+8=32-> MSB pos 5 -> block 2

  // shifting the geometric series so 2^i aligns with idx
  size_t adjusted = idx + LIST_DEFAULT_SIZE;
  size_t msb_pos = 63 - __builtin_clzll(adjusted);

  //   log2(LIST_DEFAULT_SIZE) = 3 for LIST_DEFAULT_SIZE = 8
#define LOG2_OF_LIST_DEFAULT_SIZE 3
  // first block is LIST_DEFAULT_SIZE wide, this normalizes
  r.block = msb_pos - LOG2_OF_LIST_DEFAULT_SIZE;

  size_t start_index_of_block = LIST_DEFAULT_SIZE * ((1UL << r.block) - 1);
  r.block_idx = idx - start_index_of_block;
  return r;
}

#include "common.h"

// TODO: needs collision handling asap
Map Map_new(size_t cap, Allocator *a) {
  Map m = {.entries = {0}, .cap = cap};

  // allocate block pointers array
  m.entries.blocks = CALL(a, request, LIST_BLOCK_COUNT * sizeof(Value *));
  ASSERT(m.entries.blocks != NULL, "Map_new: block array allocation failed");
  memset(m.entries.blocks, 0, LIST_BLOCK_COUNT * sizeof(Value *));

  m.entries.type_size = sizeof(Value);
  m.entries.len = 0;

  size_t remaining = cap;

  for (size_t b = 0; b < LIST_BLOCK_COUNT && remaining > 0; b++) {
    size_t block_size = LIST_DEFAULT_SIZE << b;
    size_t to_alloc = remaining < block_size ? remaining : block_size;

    m.entries.blocks[b] = CALL(a, request, to_alloc * sizeof(Value));
    ASSERT(m.entries.blocks[b] != NULL, "Map_new: block allocation failed");

    for (size_t i = 0; i < to_alloc; i++) {
      m.entries.blocks[b][i] = *INTERNED_NONE;
    }

    remaining -= to_alloc;
    m.entries.len += to_alloc;
  }

  return m;
}

void Map_insert_hash(Map *m, uint32_t hash, Value v, Allocator *a) {
  uint32_t normalized = hash % m->cap;
  LIST_insert_UNSAFE(&m->entries, normalized, v);
}

Value Map_get_hash(const Map *m, uint32_t hash) {
  uint32_t normalized = hash % m->cap;
  return LIST_get_UNSAFE(&m->entries, normalized);
}

void Map_clear(Map *m) {
  for (size_t i = 0; i < LIST_BLOCK_COUNT; i++) {
    m->entries.blocks[i] = NULL;
  }
  m->entries.len = 0;
}
