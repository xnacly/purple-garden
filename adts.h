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
  size_t block;
  // the idx into said block
  size_t block_idx;
};

struct ListIdx idx_to_block_idx(size_t idx);

#define LIST_append(LIST, ALLOC, ELEM)                                         \
  {                                                                            \
    if ((LIST)->blocks == NULL) {                                              \
      /* since LIST_new creates a zeroed instance we need to allocate the      \
       * underlying storage here*/                                             \
      (LIST)->blocks =                                                         \
          CALL(ALLOC, request, LIST_BLOCK_COUNT * sizeof(void *));             \
      ASSERT((LIST)->blocks != NULL, "LIST_new failed");                       \
      memset((LIST)->blocks, 0, LIST_BLOCK_COUNT * sizeof(void *));            \
      (LIST)->blocks[0] =                                                      \
          CALL(ALLOC, request, LIST_DEFAULT_SIZE * (LIST)->type_size);         \
      ASSERT((LIST)->blocks[0] != NULL, "LIST_new first block failed");        \
    }                                                                          \
                                                                               \
    struct ListIdx bi = idx_to_block_idx((LIST)->len);                         \
    /* append is always at the end, so this doesnt call grow(),                \
which makes sure to create all blocks before growing to its                    \
necessary size*/                                                               \
    if ((LIST)->blocks[bi.block] == NULL) {                                    \
      size_t block_size = LIST_DEFAULT_SIZE << bi.block;                       \
      (LIST)->blocks[bi.block] =                                               \
          CALL(ALLOC, request, block_size * (LIST)->type_size);                \
      ASSERT((LIST)->blocks[bi.block] != NULL,                                 \
             "List_append: allocation failed");                                \
    }                                                                          \
    (LIST)->blocks[bi.block][bi.block_idx] = ELEM;                             \
    (LIST)->len++;                                                             \
  }

#define LIST_get(LIST, IDX)                                                    \
  ({                                                                           \
    ASSERT(IDX < (LIST)->len, "List_get out of bounds");                       \
    struct ListIdx b_idx = idx_to_block_idx(IDX);                              \
    (LIST)->blocks[b_idx.block][b_idx.block_idx];                              \
  })

#define LIST_insert(LIST, ALLOC, IDX, ELEM)                                    \
  {                                                                            \
    ASSERT(IDX <= (LIST)->len, "List_insert: index out of bounds");            \
    if (IDX == (LIST)->len) {                                                  \
      LIST_append(LIST, ALLOC, ELEM);                                          \
    } else {                                                                   \
      struct ListIdx idx = idx_to_block_idx(IDX);                              \
      for (size_t b = 0; b <= idx.block; b++) {                                \
        if ((LIST)->blocks[b] == NULL) {                                       \
          size_t block_size = LIST_DEFAULT_SIZE << b;                          \
          (LIST)->blocks[b] = CALL(ALLOC, request, block_size * sizeof(ELEM)); \
          ASSERT((LIST)->blocks[b] != NULL, "List grow failed");               \
        }                                                                      \
      }                                                                        \
                                                                               \
      size_t i = (LIST)->len;                                                  \
      while (i > IDX) {                                                        \
        struct ListIdx from = idx_to_block_idx(i - 1);                         \
        struct ListIdx to = idx_to_block_idx(i);                               \
                                                                               \
        if ((LIST)->blocks[to.block] == NULL) {                                \
          size_t block_size = LIST_DEFAULT_SIZE << to.block;                   \
          (LIST)->blocks[to.block] =                                           \
              CALL(ALLOC, request, block_size * sizeof(ELEM));                 \
          ASSERT((LIST)->blocks[to.block] != NULL,                             \
                 "List_insert: allocation failed");                            \
        }                                                                      \
                                                                               \
        (LIST)->blocks[to.block][to.block_idx] =                               \
            (LIST)->blocks[from.block][from.block_idx];                        \
        i--;                                                                   \
      }                                                                        \
                                                                               \
      struct ListIdx bi = idx_to_block_idx(IDX);                               \
      (LIST)->blocks[bi.block][bi.block_idx] = ELEM;                           \
      (LIST)->len++;                                                           \
    }                                                                          \
  }

#define LIST_INSERT_UNSAFE(LIST, VAL, IDX)                                     \
  struct ListIdx idx = idx_to_block_idx(IDX);                                  \
  (LIST)->blocks[idx.block][idx.block_idx] = VAL;

#define MAP_DEFAULT_SIZE 8

// forward declared so the compiler knows a thing or
// two about a thing or two
typedef struct Value Value;
typedef struct Map Map;

Map Map_new(size_t cap, Allocator *a);
void Map_insert(Map *m, Str *s, Value v, Allocator *a);
void Map_insert_hash(Map *m, uint32_t hash, Value v, Allocator *a);
Value Map_get(const Map *m, Str *s);
Value Map_get_hash(const Map *m, uint32_t hash);
void Map_clear(Map *m);
