#include "map.h"
#include "common.h"

// hash_value computes a hash from value, the first byte of the hash is equal to
// Value.type
size_t Value_hash(Value *key, size_t max) {
  size_t h = 0;
  switch (key->type) {
  case V_OPTION:
    // should this even be an option? im not sure if i even want to include
    // theses
    break;
  case V_STRING:
    h = Str_hash(&key->string);
    break;
  case V_NUM:
    h = (size_t)key->number;
    break;
  case V_UNDEFINED:
  case V_TRUE:
  case V_FALSE:
    // these should all be unique via their hashes, true and false and undefined
    // should all point to the same index / return the same index into the
    // global pool
    break;
  case V_LIST:
    TODO("hash_value#V_LIST")
  default:
    break;
  }

  // this seems hacky, but sets the high byte to the type of the key, so
  // hashes from different types can never be equal since their types are
  // embedded
  return (h ^ ((uint64_t)key->type << 56)) % max;
}
