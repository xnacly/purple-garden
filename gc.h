#pragma once

#include "mem.h"

typedef enum {
  GC_OBJ_LIST = 1,
  GC_OBJ_MAP = 1,
  GC_OBJ_STR = 1,
} ObjType;

typedef struct GcHeader {
  unsigned int marked : 1;
  unsigned int type : 3;
  uint32_t payload;
  struct GcHeader *next;
} GcHeader;

typedef struct {
  Allocator *underlying;
  void *vm;
  GcHeader *head;
  size_t threshold;
  size_t allocated;
} Gc;

Gc gc_init(Allocator *a, void *vm, size_t threshold);
void *gc_request(Gc *gc, size_t size, ObjType t);
Stats gc_stats(Gc *gc);
void gc_cycle(Gc *gc);
