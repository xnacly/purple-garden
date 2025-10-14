// adts defines abstract datatypes for internal (runtime) and userspace (std
// packages, maps, arrays) usage
#pragma once

#include "mem.h"
#include "strings.h"
#include <string.h>

#define LIST_DEFAULT_SIZE 8
// 24 blocks means around 134mio elements, thats enough I think
#define LIST_BLOCK_COUNT 24

#define LIST_TYPE(TYPE)                                                        \
  typedef struct {                                                             \
    TYPE **blocks;                                                             \
    uint64_t len;                                                              \
    size_t type_size;                                                          \
  } LIST_##TYPE

#define LIST_new(TYPE)                                                         \
  ({                                                                           \
    LIST_##TYPE l = {0};                                                       \
    l.type_size = sizeof(TYPE);                                                \
    l;                                                                         \
  })

struct ListIdx {
  // which block to use for the indexing
  uint64_t block;
  // the idx into said block
  uint64_t block_idx;
};

struct ListIdx idx_to_block_idx(size_t idx);

#define LIST_append(LIST, ALLOC, ELEM)                                         \
  {                                                                            \
    /* allocate block array if not yet allocated */                            \
    if ((LIST)->blocks == NULL) {                                              \
      (LIST)->blocks =                                                         \
          CALL(ALLOC, request, LIST_BLOCK_COUNT * sizeof(void *));             \
      ASSERT((LIST)->blocks != NULL,                                           \
             "LIST_append: block array allocation failed");                    \
    }                                                                          \
                                                                               \
    struct ListIdx bi = idx_to_block_idx((LIST)->len);                         \
                                                                               \
    /* allocate the specific block if needed */                                \
    if ((LIST)->blocks[bi.block] == NULL) {                                    \
      uint64_t block_size = LIST_DEFAULT_SIZE << bi.block;                     \
      (LIST)->blocks[bi.block] =                                               \
          CALL(ALLOC, request, block_size * (LIST)->type_size);                \
      ASSERT((LIST)->blocks[bi.block] != NULL,                                 \
             "LIST_append: block allocation failed");                          \
    }                                                                          \
                                                                               \
    (LIST)->blocks[bi.block][bi.block_idx] = (ELEM);                           \
    (LIST)->len++;                                                             \
  }

#define LIST_get(LIST, IDX)                                                    \
  ({                                                                           \
    ASSERT(IDX < (LIST)->len, "List_get out of bounds");                       \
    struct ListIdx b_idx = idx_to_block_idx(IDX);                              \
    (LIST)->blocks[b_idx.block][b_idx.block_idx];                              \
  })

#define LIST_get_UNSAFE(LIST, IDX)                                             \
  ({                                                                           \
    struct ListIdx b_idx = idx_to_block_idx(IDX);                              \
    (LIST)->blocks[b_idx.block][b_idx.block_idx];                              \
  })

#define LIST_insert_UNSAFE(LIST, IDX, VAL)                                     \
  {                                                                            \
    struct ListIdx __idx = idx_to_block_idx(IDX);                              \
    (LIST)->blocks[__idx.block][__idx.block_idx] = VAL;                        \
  }

#define MAP_DEFAULT_SIZE 8

// forward declared so the compiler knows a thing or
// two about a thing or two
typedef struct Value Value;
typedef struct Map Map;

Map Map_new(size_t cap, Allocator *a);
void Map_insert(Map *m, const Str *s, Value v, Allocator *a);
void Map_insert_hash(Map *m, uint32_t hash, Value v);
Value Map_get(const Map *m, const Str *s);
Value Map_get_hash(const Map *m, uint32_t hash);
