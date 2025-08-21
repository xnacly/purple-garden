// adts defines abstract datatypes for internal (runtime) and userspace (std
// packages, maps, arrays) usage
#pragma once

#include "common.h"
#include "mem.h"

#define LIST_DEFAULT_CAP 8
#define LIST_GROW_MULTIPLIER 1.5f

List List_new(size_t cap, Allocator *a);
void List_append(List *l, Value v, Allocator *a);
Value List_get(const List *l, size_t idx);

#define MAP_DEFAULT_SIZE 16
#define MAP_GROW_MULTIPLIER 1.5f

Map Map_new(size_t cap, Allocator *a);
void Map_insert(Map *m, Str *s, Value v, Allocator *a);
Value *Map_get(const Map *m, Str *s);
bool Map_has(const Map *m, Str *s);
