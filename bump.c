#define _GNU_SOURCE
#include "common.h"
#include "mem.h"
#include <stdint.h>
#include <sys/mman.h>
#include <unistd.h>

// BumpResize allocator header
typedef struct {
  // points to the start of the allocated block from which Allocator::request
  // will hand out aligned chunks
  void *block;
  // the size of said allocated block
  size_t len;
  // the current amount of bytes in use
  size_t pos;
} BumpResizeCtx;

void *bump_request(void *ctx, size_t size) {
  BumpResizeCtx *b_ctx = ctx;
  size_t align = sizeof(void *);
  b_ctx->pos = (b_ctx->pos + align - 1) & ~(align - 1);

  if (b_ctx->pos + size > b_ctx->len) {
    size_t new_len = b_ctx->len * 2;
    while (new_len < b_ctx->pos + size) {
      new_len *= 2;
    }

    void *new_block = mremap(b_ctx->block, b_ctx->len, new_len, MREMAP_MAYMOVE);
    ASSERT(new_block != MAP_FAILED, "mremap failed");
    b_ctx->block = new_block;
    b_ctx->len = new_len;
  }

  void *block_entry = (char *)b_ctx->block + b_ctx->pos;
  b_ctx->pos += size;
  return block_entry;
}

void bump_destroy(void *ctx) {
  ASSERT(ctx != NULL, "bump_destroy on already destroyed allocator");
  BumpResizeCtx *b_ctx = (BumpResizeCtx *)ctx;
  madvise(b_ctx->block, b_ctx->len, MADV_FREE);
  int res = munmap(b_ctx->block, b_ctx->len);
  ASSERT(res == 0, "munmap failed");
  free(ctx);
}

Stats bump_stats(void *ctx) {
  BumpResizeCtx *b_ctx = (BumpResizeCtx *)ctx;
  return (Stats){.allocated = b_ctx->len, .current = b_ctx->pos};
}

Allocator *bump_init(size_t size) {
  long page_size = sysconf(_SC_PAGESIZE);
  size = (size + page_size - 1) & ~(page_size - 1);

  void *b = mmap(NULL, size, PROT_READ | PROT_WRITE,
                 MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
  ASSERT(b != MAP_FAILED, "failed to mmap allocator buffer");

  BumpResizeCtx *ctx = malloc(sizeof(BumpResizeCtx));
  ASSERT(ctx != NULL, "failed to bump allocator context");
  ctx->len = size;
  ctx->pos = 0;
  ctx->block = b;

  Allocator *a = malloc(sizeof(Allocator));
  ASSERT(a != NULL, "failed to alloc bump allocator");
  a->ctx = (void *)ctx;
  a->destroy = bump_destroy;
  a->request = bump_request;
  a->stats = bump_stats;

  return a;
}
