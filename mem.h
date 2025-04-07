#ifndef MEM_H
#define MEM_H

#include "common.h"

// Allocator defines an interface abstracting different allocators, so the
// runtime of the virtual machine does not need to know about implementation
// details, can be used like this:
//
typedef struct {
  // Allocator::ctx refers to an internal allocator state and owned memory
  // areas, for instance, a bump allocator would attach its meta data (current
  // position, cap, etc) here
  void *ctx;

  // Allocator::init does initial house keeping and returns the value for
  // Allocator::ctx, this MUST outlive any callsite
  void *(*init)(size_t size);
  // Allocator::request returns a handle to a block of memory of size `size`
  void *(*request)(void *ctx, size_t size);
  // Allocator::destroy cleans state up and deallocates any owned memory areas
  void (*destroy)(void *ctx);
  // Allocator::reset resets the allocator space while ideally keeping the
  // memory allocated for future use.
  void (*reset)(void *ctx);
} Allocator;

typedef struct {
  void *block;
  size_t len;
  size_t pos;
} BumpCtx;

void *bump_init(size_t size);
void *bump_request(void *ctx, size_t size);
void bump_destroy(void *ctx);
void bump_reset(void *ctx);

#endif
