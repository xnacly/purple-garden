#include "gc.h"
#include "common.h"
#include "mem.h"
#include "vm.h"
#include <stdio.h>

#define VERBOSE_GC 1

const char *GC_OBJ_TYPES[] = {
    [GC_OBJ_RAW] = "RAW",
    [GC_OBJ_STR] = "STR",
    [GC_OBJ_LIST] = "LIST",
    [GC_OBJ_MAP] = "MAP",
};

Gc gc_init(void *vm, size_t threshold) {
  return (Gc){
      .old = bump_init(GC_MIN_HEAP, 0),
      .new = bump_init(GC_MIN_HEAP, 0),
      .vm = vm,
      .head = NULL,
      .threshold = threshold,
  };
}

// Makes a gc based allocation with metadata attached at the start
void *gc_request(Gc *gc, size_t size, ObjType t) {
  void *allocation = gc->old->request(gc->old->ctx, size + sizeof(GcHeader));

  // +----------------------+ <- allocation (raw pointer)
  // | GcHeader             |  <-- header
  // +----------------------+
  // | payload (size bytes) | <-- data handed out as ptr to the user
  // +----------------------+

  void *payload = (char *)allocation + sizeof(GcHeader);
  GcHeader *h = (GcHeader *)allocation;
  h->type = t;
  h->marked = 0;
  h->size = size;
  h->payload = (uintptr_t)payload;
  h->next = gc->head;
  gc->head = h;

  gc->allocated += size;

#ifdef VERBOSE_GC
  printf("[GC] type=%s, size=%zu, header=%p, payload=%p\n", GC_OBJ_TYPES[t],
         size, (void *)h, payload);
#endif

  return (void *)payload;
}

Stats gc_stats(Gc *gc) { return CALL(gc->old, stats); }

void gc_cycle(Gc *gc) {
#ifdef VERBOSE_GC
  printf("[GC] starting cycle at %.2f%% (%zuB) heap usage\n",
         gc->allocated * 100 / (double)gc->threshold, gc->allocated);
#endif

  // walking registers
  for (size_t i = 0; i < REGISTERS + 1; i++) {
    Value *ri = ((Vm *)(gc->vm))->registers + i;
    if (!ri->is_heap) {
      continue;
    }

    // TODO: access GcHeader for these and their references
    switch ((ValueType)ri->type) {
    case V_STR:
      // TODO: heap strings contain GC_OBJ_STR, thus we need to mark these too
      break;
    case V_ARRAY:
      break;
    case V_OBJ:
      break;
    default:
      continue;
    }

#ifdef VERBOSE_GC
    printf("[GC] marked object in r%zu=", i);
    Value_debug(ri);
    puts("");
#endif
  }

  // TODO: walk root set (variable table, registers)

  for (GcHeader *h = gc->head; h; h = h->next) {
    if (!h->marked) {
#ifdef VERBOSE_GC
      printf("[GC] sweeping unreachable object at %p {.size=%ld, .type=%s}\n",
             h, h->size, GC_OBJ_TYPES[h->type]);
#endif
    }

    // TODO: copy marked from old to new

    h->marked = 0;
  }

  // CALL(gc->old, reset);
  // TODO: swap old and new
}
