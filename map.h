#ifndef MAP_H
#define MAP_H

#include "common.h"

// ValueMap maps the global pool index for a key Value inserted via
// ValueMap_insert (size_t), this is used for atom intering at compile time
typedef struct {
  size_t *values;
  size_t len;
  size_t cap;
} ValueMap;

size_t ValueMap_get(Value *key);
size_t ValueMap_insert(Value *key, size_t value);
size_t Value_hash(Value *key, size_t max);

#endif
