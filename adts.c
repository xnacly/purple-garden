#include "adts.h"

#include <stdint.h>

inline __attribute__((always_inline, hot)) struct ListIdx
idx_to_block_idx(size_t idx) {
  if (idx < LIST_DEFAULT_SIZE) {
    return (struct ListIdx){.block_idx = idx, .block = 0};
  }

  size_t adjusted = idx + LIST_DEFAULT_SIZE;
  size_t msb_pos = 63 - __builtin_clzll(adjusted);

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

  //   log2(LIST_DEFAULT_SIZE) = 3 for LIST_DEFAULT_SIZE = 8
#define LOG2_OF_LIST_DEFAULT_SIZE 3
  // first block is LIST_DEFAULT_SIZE wide, this normalizes
  size_t block = msb_pos - LOG2_OF_LIST_DEFAULT_SIZE;
  size_t start_index_of_block =
      (LIST_DEFAULT_SIZE << block) - LIST_DEFAULT_SIZE;
  size_t block_idx = idx - start_index_of_block;

  return (struct ListIdx){.block_idx = block_idx, .block = block};
}

#include "common.h"

// TODO: fix collision handling, idk how rn or what the tradeoffs are, but fix
// it.

inline Map Map_new(size_t cap, Allocator *a) {
  Map m;
  m.cap = cap;
  m.len = 0;
  m.buckets = CALL(a, request, cap * sizeof(MapEntry));
  return m;
}

static inline void Map_resize(Map *m, Allocator *a) {
  Map new = Map_new(m->cap * 2, a);

  for (size_t i = 0; i < m->cap; i++) {
    MapEntry e = m->buckets[i];
    if (e.hash != 0) {
      Map_insert_hash(&new, e.hash, e.value, a);
    }
  }

  *m = new;
}

inline void Map_clear(Map *m) {
  for (size_t i = 0; i < m->cap; i++) {
    m->buckets[i].hash = 0;
  }
  m->len = 0;
}

inline void Map_insert_hash(Map *m, uint32_t hash, Value v, Allocator *a) {
  if ((double)m->len / (double)m->cap >= 0.7) {
    Map_resize(m, a);
  }
  size_t idx = hash % m->cap;
  MapEntry *e = &m->buckets[idx];
  e->hash = hash;
  e->value = v;
}

inline Value Map_get_hash(const Map *m, uint32_t hash) {
  uint32_t idx = hash % m->cap;
  MapEntry *e = &m->buckets[idx];
  return e->value;
}

Value Map_get(const Map *m, const Str *s) {
  uint32_t hash = s->hash;
  if (hash == 0) {
    Str_hash(s);
  }
  return Map_get_hash(m, hash);
}

void Map_insert(Map *m, const Str *s, Value v, Allocator *a) {
  uint32_t hash = s->hash;
  if (hash == 0) {
    Str_hash(s);
  }
  Map_insert_hash(m, hash, v, a);
}
