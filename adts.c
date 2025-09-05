#include "adts.h"

#include <string.h>

#include "common.h"
#include "mem.h"

List List_new(size_t cap, Allocator *a) {
  List l = {
      .cap = cap < LIST_DEFAULT_CAP ? LIST_DEFAULT_CAP : cap,
      .len = 0,
  };
  l.elements = CALL(a, request, l.cap * sizeof(Value));
  return l;
}

static void grow(List *l, Allocator *a, size_t to_grow_to) {
  size_t old_cap = l->cap;
  size_t new_cap = to_grow_to;

  Value *new_mem = CALL(a, request, new_cap * sizeof(Value));
  ASSERT(new_mem != NULL, "List growth failed: %zu -> %zu", old_cap, new_cap);

  // sadly we gotta copy, since we own all values :(
  memcpy(new_mem, l->elements, old_cap * sizeof(Value));
  l->elements = new_mem;
  l->cap = new_cap;
}

void List_append(List *l, Allocator *a, Value v) {
  if (l->len >= l->cap) {
    grow(l, a, l->cap * LIST_GROW_MULTIPLIER);
  }

  l->elements[l->len++] = v;
}

Value List_get(const List *l, size_t idx) { return l->elements[idx]; }

void List_insert(List *l, Allocator *a, size_t idx, Value v) {
  // we cant insert where we havent allocated, we need to grow until we are
  // l->cap > idx
  if (l->cap < idx) {
    size_t new_cap = (size_t)(MAX(l->cap, idx + 1) * LIST_GROW_MULTIPLIER);
    grow(l, a, new_cap);
  }
  l->elements[idx] = v;
}

Map Map_new(size_t cap, Allocator *a) {
  // TODO: deal with collisions
  // TODO: figure out a good default size, 8, 16, 32, 64?
  // TODO: how does one grow a map, does every key need rehashing?
  // TODO: should the buckets really be an "array" of Lists? Or does this need a
  // different internal representation since I want to use it for things like
  // @std/encoding/json, etc; maybe namespaces need to be implemented
  // differently from V_OBJ?
  return (Map){};
}

void Map_insert(Map *m, Str *s, Value v, Allocator *a);
Value *Map_get(const Map *m, Str *s);
bool Map_has(const Map *m, Str *s);
