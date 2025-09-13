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
  m.entries = LIST_new(Value, a);
  for (size_t i = 0; i < cap; i++) {
    LIST_append(&m.entries, a, *INTERNED_NONE);
  }
  return m;
}

void Map_insert_hash(Map *m, uint32_t hash, Value v, Allocator *a) {
  uint32_t normalized = hash % m->cap;
  LIST_insert(&m->entries, a, normalized, v);
}

Value Map_get_hash(const Map *m, uint32_t hash) {
  uint32_t normalized = hash % m->cap;
  return LIST_get(&m->entries, normalized);
}

void Map_clear(Map *m) {
  for (size_t i = 0; i < LIST_BLOCK_COUNT; i++) {
    m->entries.blocks[i] = NULL;
  }
  m->entries.len = 0;
}
