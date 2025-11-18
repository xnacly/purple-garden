#pragma once

#include "mem.h"

typedef enum {
  // just bytes
  GC_OBJ_RAW = 0b000,
  // a string with a reference to an inner string, can be allocated or not
  GC_OBJ_STR = 0b001,
  // list has zero or more children
  GC_OBJ_LIST = 0b010,
  // map holds allocated buckets with owned children
  GC_OBJ_MAP = 0b100,
} ObjType;

typedef struct GcHeader {
  unsigned int marked : 1;
  unsigned int type : 3;
  uintptr_t payload;
  uintptr_t forward;
  size_t size;
  struct GcHeader *next;
} GcHeader;

// The manchester garbage collector is based on a hybrid garbage collection
// strategy merging bump allocation, mark-and-sweep, and semi-space copying.
// This makes it fast, low latency and memory efficient by allocating in
// contiguous blocks, being precise in reachability identification, relocating
// live objects into a fresh allocation space and reducing fragmentation by
// compacting live objects.
//
// The biggest downside is the slightly larger memory usage in comparison to
// other strategies, since there is a need for two memory spaces of the same
// size, and the rewriting of references after moving objects from one space to
// the other.
//
// Its works in 4 stages:
//
// 1. marking reachable memory segments by walking the roots (virtual machine
// registers, variable table)
// 2. copying marked memory from the old bump allocator space to the new bump
// allocator space
// 3. rewriting all references to the old space to point to the new space
// 4. swapping the old bump allocator space with the new one and resetting the
// old space
//
// It is specifically tuned for short lived allocations and very high allocation
// performance.
//

//     +-------------+
//     |  Roots Set  |
//     | (VM regs,   |
//     | globals...) |
//     +------+------+
//            |
//            v
//     +-------------+
//     |  Mark Phase |
//     | Mark all    |
//     | live objects|
//     +------+------+
//            |
//            v              Copy
//     +-------------------+ live     +-------------------+
//     |   Old Bump Space  | objects  |   New Bump Space  |
//     |  (old allocator)  | -------> |  (new allocator)  |
//     +-------------------+          +-------------------+
//            |
//            v
//     +--------------+
//     | Reset Old    |
//     | Bump Alloc   |
//     | (len=0,pos=0)|
//     +------+-------+
//            |
//            v
//     +-------------+
//     | Swap Alloc  |
//     | old <-> new |
//     +-------------+
//
// Its short for manchester garbage collector, since I came up with its hybrid
// approach while being on vacation in manchester.
typedef struct {
  // from-space
  Allocator *old;
  // to-space
  Allocator *new;
  void *vm;
  GcHeader *head;
  size_t threshold;
  size_t allocated;
} Gc;

Gc gc_init(void *vm, size_t threshold);
void *gc_request(Gc *gc, size_t size, ObjType t);
Stats gc_stats(Gc *gc);
void gc_cycle(Gc *gc);
