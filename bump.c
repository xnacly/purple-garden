#include <sys/types.h>
#define _GNU_SOURCE
#include "common.h"
#include "mem.h"
#include <stdint.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

// if each block is duplicated and we start at 512B, this should be fine
#define BUMP_MIN_START 512
#define BUMP_MAX_BLOCKS 28
#define BUMP_GROWTH 2

// BumpResize allocator header
//
// The bump allocator is implemented as such, so a "regrow" (needing the next
// block) doesnt invalidate all previously handed out pointers.
//
typedef struct {
  // List of blocks the bump allocator uses to hand out memory
  void *blocks[BUMP_MAX_BLOCKS];

  // the current block we are in, max is BUMP_MAX_BLOCKS
  uint64_t pos;

  // the size of the current allocated block
  uint64_t size;

  // the amount of bytes in the current block in use
  uint64_t len;

  // the max amount the bump alloc should grow to
  uint64_t max;

  uint64_t total_used;
  uint64_t total_allocated;
} BumpCtx;

void *bump_request(void *ctx, size_t size) {
  BumpCtx *b_ctx = ctx;
  size_t align = sizeof(void *);
  size_t aligned_pos = (b_ctx->len + align - 1) & ~(align - 1);

  if (aligned_pos + size > b_ctx->size) {
    ASSERT(b_ctx->pos + 1 < BUMP_MAX_BLOCKS, "Out of block size");
    uint64_t new_size = b_ctx->size * BUMP_GROWTH;
    void *new_block = malloc(new_size);
    ASSERT(new_block != NULL, "Failed to get a new bump block");
    b_ctx->blocks[++b_ctx->pos] = new_block;
    b_ctx->size = new_size;
    b_ctx->len = 0;
    aligned_pos = 0;
    b_ctx->total_allocated += new_size;
  }

  void *ptr = (char *)b_ctx->blocks[b_ctx->pos] + aligned_pos;
  b_ctx->len = aligned_pos + size;
  b_ctx->total_used += size;
  return ptr;
}

void bump_destroy(void *ctx) {
  ASSERT(ctx != NULL, "bump_destroy on already destroyed allocator");
  BumpCtx *b_ctx = (BumpCtx *)ctx;
  for (size_t i = 0; i < BUMP_MAX_BLOCKS; i++) {
    if (b_ctx->blocks[i] == NULL) {
      break;
    }
    free(b_ctx->blocks[i]);
    b_ctx->blocks[i] = NULL;
  }
  free(ctx);
}

Stats bump_stats(void *ctx) {
  BumpCtx *b_ctx = (BumpCtx *)ctx;
  return (Stats){.allocated = b_ctx->total_allocated,
                 .current = b_ctx->total_used};
}

Allocator *bump_init(size_t min_size, uint64_t max_size) {
  // TODO: reuse this once the logic is applied to memory mapping
  // long page_size = sysconf(_SC_PAGESIZE);
  // size_t size = (min_size + page_size - 1) & ~(page_size - 1);

  // void *b = mmap(NULL, size, PROT_READ | PROT_WRITE,
  //                MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
  // ASSERT(b != MAP_FAILED, "failed to mmap allocator buffer");

  BumpCtx *ctx = malloc(sizeof(BumpCtx));
  ASSERT(ctx != NULL, "failed to bump allocator context");
  *ctx = (BumpCtx){};
  ctx->size = min_size < BUMP_MIN_START ? BUMP_MIN_START : min_size;
  ctx->max = max_size;
  ctx->blocks[0] = malloc(ctx->size);
  ctx->total_allocated += ctx->size;
  ASSERT(ctx->blocks[0] != NULL, "Failed to allocate initial bump block");

  Allocator *a = malloc(sizeof(Allocator));
  ASSERT(a != NULL, "failed to alloc bump allocator");
  a->ctx = (void *)ctx;
  a->destroy = bump_destroy;
  a->request = bump_request;
  a->stats = bump_stats;

  return a;
}
