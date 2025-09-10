// adts defines abstract datatypes for internal (runtime) and userspace (std
// packages, maps, arrays) usage
#pragma once

#include "common.h"
#include "mem.h"

#define LIST_DEFAULT_SIZE 8

List List_new(Allocator *a);
List List_new_with_size(uint32_t cap, Allocator *a);
void List_append(List *l, Allocator *a, Value v);
Value List_get(const List *l, size_t idx);
void List_insert(List *l, Allocator *a, size_t idx, Value v);

#define MAP_DEFAULT_SIZE 8

Map Map_new(size_t cap, Allocator *a);
void Map_insert(Map *m, Str *s, Value v, Allocator *a);
Value *Map_get(const Map *m, Str *s);
bool Map_has(const Map *m, Str *s);
