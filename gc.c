#include "gc.h"

Gc gc_init(Allocator *a, void *vm, size_t threshold) {
  return (Gc){
      .underlying = a,
      .vm = vm,
      .head = NULL,
      .threshold = threshold,
  };
}

void *gc_request(Gc *gc, size_t size, ObjType t) {
  return CALL(gc->underlying, request, size);
}

Stats gc_stats(Gc *gc) { return CALL(gc->underlying, stats); }

void gc_cycle(Gc *gc) {}
