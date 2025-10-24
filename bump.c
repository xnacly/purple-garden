#define _GNU_SOURCE
#include "common.h"
#include "mem.h"
#include <stdint.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/types.h>
#include <unistd.h>

#define BUMP_MIN_START 4096
// geometric series result for max amount of bytes fitting into uint64_t used to
// count the totally allocated bytes; amounts to something like (2^64)-
#define BUMP_MAX_BLOCKS 55
#define BUMP_GROWTH 2

// BumpResize allocator header
//
// The bump allocator is implemented as such, so a "regrow" (needing the next
// block) doesnt invalidate all previously handed out pointers. ALWAYS zero all
// handed out memory yourself
typedef struct {
  // the current block we are in, max is BUMP_MAX_BLOCKS
  uint64_t pos;

  // the size of the current allocated block
  uint64_t size;

  // the amount of bytes in the current block in use
  uint64_t len;

  // the max amount the bump alloc should grow to
  uint64_t max;

  // kept for Allocator->stats
  uint64_t total_used;
  uint64_t total_allocated;

  // List of blocks the bump allocator uses to hand out memory
  void *blocks[BUMP_MAX_BLOCKS];
  uint64_t block_sizes[BUMP_MAX_BLOCKS];
} BumpCtx;

void *bump_request(void *ctx, size_t size) {
  BumpCtx *b_ctx = ctx;
  if (b_ctx->pos >= BUMP_MAX_BLOCKS) {
    printf("b_ctx->pos out of range: %lu\n", b_ctx->pos);
    return NULL;
  }
  size_t align = sizeof(void *);
  uint64_t aligned_pos = (b_ctx->len + align - 1) & ~(align - 1);

  if (b_ctx->max > 0) {
    ASSERT(b_ctx->total_allocated < b_ctx->max,
           "Bump allocator exceeded max_size");
  }

  if (aligned_pos + size > b_ctx->size) {
    ASSERT(b_ctx->pos + 1 < BUMP_MAX_BLOCKS, "Out of block size");
    uint64_t new_size = b_ctx->size * BUMP_GROWTH;

    void *new_block = mmap(NULL, new_size, PROT_READ | PROT_WRITE,
                           MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    ASSERT(new_block != MAP_FAILED, "Failed to mmap new block");

    b_ctx->blocks[++b_ctx->pos] = new_block;
    b_ctx->block_sizes[b_ctx->pos] = new_size;
    b_ctx->size = new_size;
    b_ctx->len = 0;
    aligned_pos = 0;
    b_ctx->total_allocated += new_size;
  }

  void *ptr = (char *)b_ctx->blocks[b_ctx->pos] + aligned_pos;
  b_ctx->total_used += (aligned_pos - b_ctx->len) + size;
  b_ctx->len = aligned_pos + size;
  return ptr;
}

void bump_destroy(void *ctx) {
  ASSERT(ctx != NULL, "bump_destroy on already destroyed allocator");
  BumpCtx *b_ctx = (BumpCtx *)ctx;
  for (size_t i = 0; i <= b_ctx->pos; i++) {
    if (b_ctx->blocks[i]) {
      munmap(b_ctx->blocks[i], b_ctx->block_sizes[i]);
      b_ctx->blocks[i] = NULL;
    }
  }
  free(ctx);
}

Stats bump_stats(void *ctx) {
  BumpCtx *b_ctx = (BumpCtx *)ctx;
  return (Stats){.allocated = b_ctx->total_allocated,
                 .current = b_ctx->total_used};
}

Allocator *bump_init(uint64_t min_size, uint64_t max_size) {
  BumpCtx *ctx = malloc(sizeof(BumpCtx));
  ASSERT(ctx != NULL, "failed to bump allocator context");
  *ctx = (BumpCtx){0};
  ctx->size = min_size < BUMP_MIN_START ? BUMP_MIN_START : min_size;
  ctx->max = max_size;
  void *first_block = mmap(NULL, ctx->size, PROT_READ | PROT_WRITE,
                           MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
  ASSERT(first_block != MAP_FAILED, "Failed to mmap initial block");
  ctx->blocks[0] = first_block;
  ctx->total_allocated = ctx->size;

  Allocator *a = malloc(sizeof(Allocator));
  ASSERT(a != NULL, "failed to alloc bump allocator");
  *a = (Allocator){0};
  a->ctx = (void *)ctx;
  a->destroy = bump_destroy;
  a->request = bump_request;
  a->stats = bump_stats;

  return a;
}
