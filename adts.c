#include "adts.h"

#include "common.h"
#include "mem.h"
#include <string.h>

List List_new(size_t cap, Allocator *a) {
  List l = {
      .cap = cap < LIST_DEFAULT_CAP ? LIST_DEFAULT_CAP : cap,
      .len = 0,
  };
  l.elements = CALL(a, request, l.cap * sizeof(Value));
  return l;
}

void List_append(List *l, Value v, Allocator *a) {
  if (l->len + 1 >= l->cap) {
    size_t old_cap = l->cap;
    size_t new_cap = old_cap * 2;

    Value *new_mem = CALL(a, request, new_cap * sizeof(Value));
    ASSERT(new_mem != NULL, "List growth failed: %zu -> %zu", old_cap, new_cap);

    memcpy(new_mem, l->elements, old_cap * sizeof(Value));
    l->elements = (struct Value *)new_mem;
    l->cap = new_cap;
  }

  ((Value *)l->elements)[l->len++] = v;
}

// Option guarded checked access to the inner elements
Value *List_get(const List *l, size_t idx) {
  if (idx >= l->len) {
    return INTERNED_NONE;
  }

  return &((Value *)l->elements)[idx];
}

// TODO: implement before adding support for V_OBJ
Map Map_new(size_t cap, Allocator *a);
void Map_insert(Map *m, Str *s, Value v, Allocator *a);
Value *Map_get(const Map *m, Str *s);
bool Map_has(const Map *m, Str *s);
