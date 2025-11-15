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
  printf("[GC][REQU] type=%s, size=%zu, payload=%p\n", GC_OBJ_TYPES[t], size,
         payload);
#endif

  return (void *)payload;
}

Stats gc_stats(Gc *gc) {
  Stats old = CALL(gc->old, stats);
  Stats new = CALL(gc->new, stats);
  return (Stats){
      .current = old.current + new.current,
      .allocated = old.allocated + new.allocated,
  };
}

static inline void *forward_ptr(void *payload) {
  if (!payload) {
    return NULL;
  }

  GcHeader *old = (GcHeader *)((char *)payload - sizeof(GcHeader));
  if (!old) {
    // not a gc object, hitting this is a bug
    unreachable();
  }

  if (old->forward) {
    return (void *)old->forward;
  }

  // normally this would be unreachable, but since pg doesnt clear registers
  // after insertions into adts or the variable table, references to heap data
  // can be both in the variable table and registers at the same time, thus
  // allowing for multiple forward_ptr calls since there are multiple references
  // to a single point in memory. This results in double forwarding and other
  // shenanigans. Just returning the payload if no forward was found is correct
  // and a fix.
  return payload;
}

static inline void rewrite(Gc *gc, Value *v) {
  if (!v->is_heap) {
    return;
  }

  switch ((ValueType)v->type) {
  case V_STR:
    v->string->p = (const uint8_t *)forward_ptr((void *)v->string->p);
    v->string = (Str *)forward_ptr((void *)v->string);
    break;
  case V_ARRAY:
    v->array = (List *)forward_ptr((void *)v->array);
    break;
  case V_OBJ:
    v->obj = (Map *)forward_ptr((void *)v->obj);
    break;
  default:
    return;
  }
}

static inline void mark(Gc *gc, const Value *val) {
  if (!val || !val->is_heap) {
    return;
  }

  void *payload = NULL;
  switch ((ValueType)val->type) {
  case V_STR:
    payload = (void *)val->string;
    break;
  case V_ARRAY:
    payload = (void *)val->array;
    break;
  case V_OBJ:
    payload = (void *)val->obj;
    break;
  default:
    return;
  }

  GcHeader *h = (GcHeader *)((char *)payload - sizeof(GcHeader));
  if (!h || h->marked) {
    return;
  }

  h->marked = 1;

  switch ((ObjType)h->type) {
  case GC_OBJ_STR: {
    // a heap string is made up of both the string view and its inner buffer
    // holding the actual bytes, GC_OBJ_STR and GC_OBJ_RAW respectively
    Str *s = (Str *)payload;
    // mark embedded raw bytes
    GcHeader *raw = (GcHeader *)((char *)s->p - sizeof(GcHeader));
    raw->marked = true;
    break;
  }
  case GC_OBJ_LIST: {
    List *l = (List *)payload;
    for (size_t i = 0; i < l->len; i++) {
      mark(gc, &l->arr[i]);
    }
    break;
  }
  case GC_OBJ_MAP:
    ASSERT(0, "V_OBJ marking not implemented yet");
  default:
    return;
  }

#ifdef VERBOSE_GC
  printf("[GC][MARK] marked value at %p::", (void *)h->payload);
  Value_debug(val);
  puts("");
#endif
};

void gc_cycle(Gc *gc) {
  Vm *vm = ((Vm *)gc->vm);
#ifdef VERBOSE_GC
  printf("[GC][MARK] starting marking at %.2f%% (%zuB) heap usage\n",
         gc->allocated * 100 / (double)gc->threshold, gc->allocated);
#endif

#ifdef VERBOSE_GC
  puts("[GC][MARK]ing registers");
#endif
  for (size_t i = 0; i < REGISTERS + 1; i++) {
    const Value *ri = vm->registers + i;
    mark(gc, ri);
  }

#ifdef VERBOSE_GC
  puts("[GC][MARK]ing variable table");
#endif
  // TODO: this needs to walk up the chain, meaning all variables contained in
  // the frames before this frame have to be marked too
  for (size_t i = 0; i < vm->frame->variable_table.cap; i++) {
    MapEntry *me = &vm->frame->variable_table.buckets[i];
    if (!me->hash) {
      continue;
    }
    mark(gc, &me->value);
  }

  GcHeader *new_head = NULL;
  size_t new_alloc = 0;
  for (GcHeader *h = gc->head; h; h = h->next) {
    if (!h->marked || h->forward) {
      continue;
    }

#ifdef VERBOSE_GC
    printf("[GC][COPY] object at %p to newspace {.size=%ld, "
           ".type=%s}\n",
           (void *)h->payload, h->size, GC_OBJ_TYPES[h->type]);
#endif

    void *buf = CALL(gc->new, request, h->size + sizeof(GcHeader));
    GcHeader *nh = (GcHeader *)buf;
    void *new_payload = (char *)buf + sizeof(GcHeader);
    memcpy(nh, h, sizeof(GcHeader) + h->size);
    nh->next = new_head;
    new_head = nh;
    h->forward = (uintptr_t)new_payload;
    nh->payload = (uintptr_t)new_payload;
    nh->forward = 0;
    nh->marked = 0;
    new_alloc += h->size;
  }

  // rewriting all alive values to point to the newspace

  for (size_t i = 0; i < REGISTERS + 1; i++) {
    Value *ri = &vm->registers[i];
    rewrite(gc, ri);
  }

  for (size_t i = 0; i < vm->frame->variable_table.cap; i++) {
    MapEntry *me = &vm->frame->variable_table.buckets[i];
    if (!me->hash) {
      continue;
    }
    rewrite(gc, &me->value);
  }

  for (GcHeader *h = new_head; h; h = h->next) {
    switch (h->type) {
    case GC_OBJ_LIST: {
      List *l = (List *)h->payload;
      for (size_t i = 0; i < l->len; i++) {
        rewrite(gc, &l->arr[i]);
      }
      break;
    }
    case GC_OBJ_MAP: {
      Map *m = (Map *)h->payload;
      for (size_t i = 0; i < m->cap; i++) {
        MapEntry *me = &m->buckets[i];
        if (me->hash) {
          rewrite(gc, &me->value);
        }
      }
      break;
    }
    case GC_OBJ_STR: {
      Str *str = (Str *)h->payload;
      str->p = forward_ptr((void *)str->p);
      break;
    }
    default:
      break;
    }
  }

  gc->head = new_head;
  CALL(gc->old, reset);
  Allocator *swap = gc->new;
  gc->new = gc->old;
  gc->old = swap;
#ifdef VERBOSE_GC
  printf("[GC][FIN] all cycles done, cleaned %zuB of %zuB used up (%.2f%%)\n",
         gc->allocated - new_alloc, gc->allocated,
         (gc->allocated - new_alloc) * 100 / (double)gc->allocated);
#endif
  gc->allocated = new_alloc;
}
