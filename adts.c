#include "adts.h"

#include <string.h>

#include "common.h"
#include "mem.h"

#define LIST_GROWTH_FACTOR 2
#define UPPER_BOUND(i) (LIST_DEFAULT_SIZE * (1 << (i)))

struct ListIdx {
  // which block to use for the indexing
  size_t block;
  // the idx into said block
  size_t block_idx;
};

static struct ListIdx idx_to_block_idx(const List *l, size_t idx) {
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

List List_new(Allocator *a) {
  // A new List is always size of LIST_DEFAULT_SIZE, since thats the currently
  // easiest way for me to program this, thus:
  //
  // TODO: create a List_new_with_size() method replacing previously removed
  // List.cap
  List l = {
      .len = 0,
  };

  l.blocks[0] = CALL(a, request, LIST_DEFAULT_SIZE * sizeof(Value));
  ASSERT(l.blocks[0] != NULL, "List_new failed");
  return l;
}

static void grow(List *l, Allocator *a, size_t to_grow_to) {
  struct ListIdx idx = idx_to_block_idx(l, to_grow_to);
  for (size_t b = 0; b <= idx.block; b++) {
    if (l->blocks[b] == NULL) {
      size_t block_size = LIST_DEFAULT_SIZE << b;
      l->blocks[b] = CALL(a, request, block_size * sizeof(Value));
      ASSERT(l->blocks[b] != NULL, "List grow failed");
    }
  }
}

void List_append(List *l, Allocator *a, Value v) {
  struct ListIdx bi = idx_to_block_idx(l, l->len);

  // append is always at the end, so this doesnt call grow(), which makes sure
  // to create all blocks before growing to its necessary size
  if (l->blocks[bi.block] == NULL) {
    size_t block_size = LIST_DEFAULT_SIZE << bi.block;
    l->blocks[bi.block] = CALL(a, request, block_size * sizeof(Value));
    ASSERT(l->blocks[bi.block] != NULL, "List_append: allocation failed");
  }

  l->blocks[bi.block][bi.block_idx] = v;
  l->len++;
}

Value List_get(const List *l, size_t idx) {
  ASSERT(idx < l->len, "List_get out of bounds");

  struct ListIdx b_idx = idx_to_block_idx(l, idx);
  Value *block = l->blocks[b_idx.block];
  ASSERT(block != NULL, "List_get: block not allocated");

  return block[b_idx.block_idx];
}

void List_insert(List *l, Allocator *a, size_t idx, Value v) {
  ASSERT(idx <= l->len, "List_insert: index out of bounds");
  if (idx == l->len) {
    List_append(l, a, v);
    return;
  }

  grow(l, a, l->len);

  // moving all other elements to the right, because insertion is heavy
  size_t i = l->len;
  while (i > idx) {
    struct ListIdx from = idx_to_block_idx(l, i - 1);
    struct ListIdx to = idx_to_block_idx(l, i);

    // Ensure target block is allocated
    if (l->blocks[to.block] == NULL) {
      size_t block_size = LIST_DEFAULT_SIZE << to.block;
      l->blocks[to.block] = CALL(a, request, block_size * sizeof(Value));
      ASSERT(l->blocks[to.block] != NULL, "List_insert: allocation failed");
    }

    l->blocks[to.block][to.block_idx] = l->blocks[from.block][from.block_idx];
    i--;
  }

  struct ListIdx bi = idx_to_block_idx(l, idx);
  l->blocks[bi.block][bi.block_idx] = v;
  l->len++;
}

Map Map_new(size_t cap, Allocator *a) {
  // TODO: deal with collisions
  // TODO: figure out a good default size, 8, 16, 32, 64?
  // TODO: how does one grow a map, does every key need rehashing?
  // TODO: should the buckets really be an "array" of Lists? Or does this need a
  // different internal representation since I want to use it for things like
  // @std/encoding/json, etc; maybe namespaces need to be implemented
  // differently from V_OBJ?
  return (Map){};
}

void Map_insert(Map *m, Str *s, Value v, Allocator *a);
Value *Map_get(const Map *m, Str *s);
bool Map_has(const Map *m, Str *s);
