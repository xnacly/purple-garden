#include "gc.h"
#include "common.h"
#include "mem.h"
#include "vm.h"
#include <stdint.h>
#include <stdio.h>

const char *GC_OBJ_TYPES[] = {
    [GC_OBJ_RAW] = "RAW",
    [GC_OBJ_LIST] = "LIST",
    [GC_OBJ_MAP] = "MAP",
};

Gc gc_init(size_t gc_size) {
  size_t half = gc_size / 2;
  return (Gc){
      .old = bump_init(half, 0),
      .new = bump_init(half, 0),
      .head = NULL,
  };
}

// Makes a gc based allocation with metadata attached at the start
void *gc_request(Gc *gc, size_t size, ObjType t) {
  void *allocation = gc->old->request(gc->old->ctx, size + sizeof(GcHeader));

  // +----------------------+ <- allocation (raw pointer)
  // | GcHeader             | <-- header
  // +----------------------+
  // |                      |
  // | payload (size B)     | <-- data handed out as ptr to the user
  // |                      |
  // +----------------------+

  void *payload = (char *)allocation + sizeof(GcHeader);
  GcHeader *h = (GcHeader *)allocation;
  h->type = t;
  h->marked = 0;
  h->size = size;
  h->next = gc->head;
  gc->head = h;

  gc->allocated_since_last_cycle += size;
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

  if (old->type < GC_OBJ_RAW || old->type > GC_OBJ_MAP) {
    // either already in newspace or not a heap object; return payload unchanged
    return payload;
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
    v->string.p = (const uint8_t *)forward_ptr((void *)v->string.p);
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

void rewrite_nested(Gc *gc, Value *v) {
  rewrite(gc, v);

  switch (v->type) {
  case V_ARRAY:
    for (size_t i = 0; i < v->array->len; i++) {
      rewrite_nested(gc, &v->array->arr[i]);
    }
    break;
  case V_OBJ:
    for (size_t i = 0; i < v->obj->cap; i++) {
      MapEntry *me = &v->obj->buckets[i];
      if (me->hash) {
        rewrite_nested(gc, &me->value);
      }
    }
    break;
  default:
    break;
  }
}

static inline void mark(Gc *gc, const Value *val) {
  if (!val || !val->is_heap) {
    return;
  }

  void *payload = NULL;
  switch ((ValueType)val->type) {
  case V_STR:
    // a heap string is made up of both the string view and its inner buffer
    // holding the actual bytes GC_OBJ_RAW respectively
    GcHeader *raw =
        (GcHeader *)((char *)((Str *)payload)->p - sizeof(GcHeader));
    raw->marked = true;
    break;
    return;
  case V_ARRAY:
    payload = (void *)val->array;
    break;
  case V_OBJ:
    payload = (void *)val->obj;
    break;
  default:
    return;
  }

  ASSERT((uintptr_t)payload > sizeof(GcHeader),
         "payload too small, GC logic bug, this shouldnt happen");

  GcHeader *h = (GcHeader *)((char *)payload - sizeof(GcHeader));
  if (!h || h->marked) {
    return;
  }

  h->marked = 1;

  switch ((ObjType)h->type) {
  case GC_OBJ_LIST: {
    List *l = (List *)payload;
    for (size_t i = 0; i < l->len; i++) {
      mark(gc, &l->arr[i]);
    }
    break;
  }
  case GC_OBJ_MAP:
    Map *m = (Map *)payload;
    for (size_t i = 0; i < m->cap; i++) {
      MapEntry e = m->buckets[i];
      mark(gc, &e.value);
    }
  default:
    return;
  }

#ifdef VERBOSE_GC
  printf("[GC][MARK] marked value at %p::", (void *)payload);
  Value_debug(val);
  puts("");
#endif
};

void gc_cycle(Gc *gc) {
  if (!gc->allocated_since_last_cycle) {
    return;
  }
  gc->allocated_since_last_cycle = 0;
  Vm *vm = ((Vm *)gc->vm);
#ifdef VERBOSE_GC
  printf("[GC][MARK] starting marking at %.2f%% (%zuB) heap usage\n",
         gc->allocated * 100 /
             (double)(CALL(gc->old, stats).allocated +
                      CALL(gc->new, stats).allocated),
         gc->allocated);
#endif

#ifdef VERBOSE_GC
  puts("[GC][MARK]ing registers");
#endif
  for (size_t i = 0; i < REGISTERS; i++) {
    const Value *ri = vm->registers + i;
    mark(gc, ri);
  }

#ifdef VERBOSE_GC
  puts("[GC][MARK]ing variable table");
#endif

  for (Frame *f = vm->frame; f; f = f->prev) {
#ifdef VERBOSE_GC
    printf("[GC][MARK] walking variable table %p {.return_to_bytecode=%zu}\n",
           f, f->return_to_bytecode);
#endif
    for (size_t i = 0; i < f->variable_table.cap; i++) {
      MapEntry *me = &f->variable_table.buckets[i];
      if (me->hash) {
        mark(gc, &me->value);
      }
    }
  }

  GcHeader *new_head = NULL;
  size_t new_alloc = 0;
  for (GcHeader *h = gc->head; h; h = h->next) {
    if (!h->marked) {
      continue;
    }

#ifdef VERBOSE_GC
    printf("[GC][COPY] object at %p to newspace {.size=%hu, "
           ".type=%s}\n",
           (void *)h, h->size, GC_OBJ_TYPES[h->type]);
#endif

    void *buf = CALL(gc->new, request, h->size + sizeof(GcHeader));
    GcHeader *nh = (GcHeader *)buf;
    void *new_payload = (char *)buf + sizeof(GcHeader);
    memcpy(nh, h, sizeof(GcHeader) + h->size);
    nh->next = new_head;
    new_head = nh;
    h->forward = (uintptr_t)new_payload;
    nh->forward = 0;
    nh->marked = 0;
    new_alloc += h->size;
  }

  // rewriting all alive values to point to the newspace
  for (size_t i = 0; i < REGISTERS; i++) {
    Value *ri = &vm->registers[i];
    rewrite(gc, ri);
  }

  for (Frame *f = vm->frame; f; f = f->prev) {
    for (size_t i = 0; i < f->variable_table.cap; i++) {
      MapEntry *me = &f->variable_table.buckets[i];
      if (me->hash) {
        rewrite_nested(gc, &me->value);
      }
    }
  }

  for (GcHeader *h = new_head; h; h = h->next) {
    switch (h->type) {
    case GC_OBJ_LIST: {
      List *l = (List *)((uint8_t *)h + sizeof(GcHeader));
      for (size_t i = 0; i < l->len; i++) {
        rewrite_nested(gc, &l->arr[i]);
      }
      break;
    }
    case GC_OBJ_MAP: {
      Map *m = (Map *)((uint8_t *)h + sizeof(GcHeader));
      for (size_t i = 0; i < m->cap; i++) {
        MapEntry *me = &m->buckets[i];
        if (me->hash) {
          rewrite_nested(gc, &me->value);
        }
      }
      break;
    }
    case GC_OBJ_STR: {
      Str *str = (Str *)((uint8_t *)h + sizeof(GcHeader));
      str->p = forward_ptr((void *)str->p);
      break;
    }
    default:
      break;
    }
  }

  gc->head = new_head;
  SWAP_STRUCT(gc->old, gc->new);
  CALL(gc->new, reset);
#ifdef VERBOSE_GC
  printf("[GC][FIN] all cycles done, cleaned %zuB of %zuB used up (%.2f%%)\n",
         gc->allocated - new_alloc, gc->allocated,
         (gc->allocated - new_alloc) * 100 / (double)gc->allocated);
#endif
  gc->allocated = new_alloc;
}
