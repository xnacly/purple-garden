#include "adts.h"

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

void List_append(List *l, Value v) {
  if (l->len + 1 >= l->cap) {
    ASSERT(0, "TODO: grow here");
  }

  ((Value *)l->elements)[l->len++] = v;
}

// Option guarded checked access to the inner elements
Value *List_get(const List *l, size_t idx) {
  if (idx >= l->len) {
    return INTERNED_NONE;
  }

  Value *e = &((Value *)l->elements)[idx];
  e->is_some = true;
  return e;
}

Map Map_new(size_t cap, Allocator *a);
void Map_insert(Map *m, Str *s, Value v);
Value *Map_get(const Map *m, Str *s);
bool Map_has(const Map *m, Str *s);
