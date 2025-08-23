#include "common.h"
#include "mem.h"
#include "vm.h"

#include <stdlib.h>

typedef struct {
  // used for the marking stage to start the scanning at the roots: registers
  // and variable table
  Vm *vm;

  void *from_space;
  void *to_space;

  size_t size;
  size_t pos;
} GcCtx;

static void xcgc_run(GcCtx *c) {
  // TODO: 1. add a bump allocator with 2kb for frames, separate from the gc
  // TODO: 2. add the Str tag for heap strings
  // TODO: 3. scan roots (variable table entries, registers)
  ASSERT(0, "xcgc collection is not implemented yet")
}

void *gc_request(void *ctx, size_t size) {
  GcCtx *c = (GcCtx *)ctx;

  // the request is now failable because we want to stay under MAX_MEM
  if (c->pos + size >= MAX_MEM) {
    return NULL;
  }

  if (c->pos + size >= c->size) {
#ifdef VERBOSE_ALLOCATOR
    printf("[XCGC] triggering gc at %zu, %.3f%% of %zuB because of %zuB\n",
           c->pos, (c->pos * 100) / (double)c->size, c->size, size);
#endif
    xcgc_run(c);

    ASSERT(c->pos < c->size, "xcgc bug: full heap after collection");
    // necessary to guard buffer overflows
  }

  size_t align = sizeof(void *);
  c->pos = (c->pos + align - 1) & ~(align - 1);
  void *p = (char *)c->to_space + c->pos;
  c->pos += size;

#ifdef VERBOSE_ALLOCATOR
  double avail = c->size - c->pos;
  printf(
      "[XCGC] allocated %zuB, %.0fB::%.3f%% available until gc is triggered\n",
      size, avail, (avail * 100) / (double)c->size);
#endif

  return p;
}

void gc_destroy(void *ctx) {
  ASSERT(ctx != NULL, "gc_destroy on already destroyed allocator");
  GcCtx *c = (GcCtx *)ctx;
  free(c->to_space);
  free(c->from_space);
  free(ctx);
}

Stats gc_stats(void *ctx) {
  GcCtx *c = (GcCtx *)ctx;
  return (Stats){.allocated = c->size, .current = c->pos};
}

// xcgc.c is my first attempt at creating a nonrecursive compacting
// algorithm based on Cheney's copying collector - now xnacly's copying
// collector (xcgc), see: https://dl.acm.org/doi/10.1145/362790.362798 and
// https://en.wikipedia.org/wiki/Cheney%27s_algorithm. With mostly separate
// stages and only stopping the world for the smalles time possible,
// specifically only while moving from the from-space to the to-space.
Allocator *xcgc_init(size_t size, void *vm) {
  GcCtx *ctx = malloc(sizeof(GcCtx));
  ASSERT(ctx != NULL, "failed to allocate garbage collector context");
  ctx->vm = (Vm *)vm;
  ctx->from_space = malloc(size);
  ASSERT(ctx->from_space != NULL, "failed to allocate xcgc from_space");
  ctx->to_space = malloc(size);
  ASSERT(ctx->to_space != NULL, "failed to allocate xcgc to_space");
  ctx->pos = 0;
  ctx->size = size;

  Allocator *a = malloc(sizeof(Allocator));
  ASSERT(ctx != NULL, "failed to alloc gc allocator");
  a->ctx = (void *)ctx;
  a->destroy = gc_destroy;
  a->request = gc_request;
  a->stats = gc_stats;
  return a;
}
