#include "mem.h"
#include "common.h"

void *bump_init(size_t size) {
  void *b = malloc(size);
  BumpCtx *ctx = malloc(sizeof(BumpCtx));
  ctx->len = size;
  ctx->pos = 0;
  ctx->block = b;
  return ctx;
}

void *bump_request(void *ctx, size_t size) {
  BumpCtx *b_ctx = (BumpCtx *)ctx;
  if (b_ctx->pos + size > b_ctx->len) {
    ASSERT(0, "OOM :(")
  }
  // INFO: maybe align this?
  // size_t align = sizeof(void *);
  // b_ctx->pos = (b_ctx->pos + align - 1) & ~(align - 1);
  void *block_entry = (char *)b_ctx->block + b_ctx->pos;
  b_ctx->pos += size;
  return block_entry;
}

void bump_destroy(void *ctx) {
  BumpCtx *b_ctx = (BumpCtx *)ctx;
#if DEBUG
  printf("bump: used %zub from %zub\n", b_ctx->pos, b_ctx->len);
#endif
  free(b_ctx->block);
  free(ctx);
}

void bump_reset(void *ctx) {
  BumpCtx *b_ctx = (BumpCtx *)ctx;
  b_ctx->pos = 0;
}
