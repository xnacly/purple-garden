#define _GNU_SOURCE
#include "common.h"
#include "mem.h"
#include <sys/mman.h>
#include <unistd.h>

// Bump allocator header
typedef struct {
  // points to the start of the allocated block from which Allocator::request
  // will hand out aligned chunks
  void *block;
  // the size of said allocated block
  size_t len;
  // the current amount of bytes in use
  size_t pos;
} BumpCtx;

void *bump_request(void *ctx, size_t size) {
  BumpCtx *b_ctx = (BumpCtx *)ctx;
  size_t align = sizeof(void *);
  b_ctx->pos = (b_ctx->pos + align - 1) & ~(align - 1);
  ASSERT(b_ctx->pos + size <= b_ctx->len, "OOM :( with %zu", b_ctx->len);
  void *block_entry = (char *)b_ctx->block + b_ctx->pos;
  b_ctx->pos += size;
  return block_entry;
}

void bump_destroy(void *ctx) {
  ASSERT(ctx != NULL, "bump_destroy on already destroyed allocator");
  BumpCtx *b_ctx = (BumpCtx *)ctx;
  // madvise(2):
  // The  application  no  longer requires the pages in the range
  // specified by addr and len. The kernel can thus  free  these
  // pages,  but  the freeing could be delayed until memory pres‐
  // sure occurs.
  //
  // TODO: benchmark this (only interesting for dealloction performance)
  madvise(b_ctx->block, b_ctx->len, MADV_FREE);
  int res = munmap(b_ctx->block, b_ctx->len);
  ASSERT(res == 0, "munmap failed");
  free(ctx);
}

Stats bump_stats(void *ctx) {
  BumpCtx *b_ctx = (BumpCtx *)ctx;
  return (Stats){.allocated = b_ctx->len, .current = b_ctx->pos};
}

Allocator *bump_init(size_t size) {
  long page_size = sysconf(_SC_PAGESIZE);
  size = (size + page_size - 1) & ~(page_size - 1);

  void *b = mmap(NULL, size, PROT_READ | PROT_WRITE,
                 MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
  ASSERT(b != MAP_FAILED, "failed to mmap allocator buffer");

  BumpCtx *ctx = malloc(sizeof(BumpCtx));
  ASSERT(ctx != NULL, "failed to bump allocator context");
  ctx->len = size;
  ctx->pos = 0;
  ctx->block = b;

  Allocator *a = malloc(sizeof(Allocator));
  ASSERT(ctx != NULL, "failed to alloc bump allocator");
  a->ctx = (void *)ctx;
  a->destroy = bump_destroy;
  a->request = bump_request;
  a->stats = bump_stats;

  return a;
}
