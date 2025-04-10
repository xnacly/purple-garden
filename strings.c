#include "strings.h"
#include "common.h"
#include <string.h>

char Str_get(const Str *str, size_t index) {
  if (str == NULL || index >= str->len - 1) {
    return -1;
  }
  return (unsigned int)str->p[index];
}

Str Str_from(const char *s) {
  return (Str){
      .len = strlen(s),
      .p = s,
  };
}

Str Str_slice(const Str *str, size_t start, size_t end) {
  ASSERT(end >= start, "Str_slice: Invalid slice range: end must be >= start");
  ASSERT(end <= str->len, "Str_slice: Slice range exceeds string length");

  return (Str){
      .p = str->p + start,
      .len = end - start,
  };
}

bool Str_eq(const Str *a, const Str *b) {
  ASSERT(a != NULL, "Str_eq: a is NULL");
  ASSERT(b != NULL, "Str_eq: b is NULL");
  if (a->len != b->len) {
    return false;
  }

  return 0 == memcmp(a->p, b->p, a->len);
}

void Str_debug(const Str *str) { printf("%.*s", (int)str->len, str->p); }

inline size_t Str_hash(const Str *str) {
  // https://en.wikipedia.org/wiki/Fowler%E2%80%93Noll%E2%80%93Vo_hash_function#FNV-1a_hash
  // https://en.wikipedia.org/wiki/Fowler%E2%80%93Noll%E2%80%93Vo_hash_function#FNV_hash_parameters
#define FNV_OFFSET_BASIS 0x811c9dc5
#define FNV_PRIME 0x01000193
  size_t hash = FNV_OFFSET_BASIS;
  for (size_t i = 0; i < str->len; i++) {
    hash ^= str->p[i];
    hash *= FNV_PRIME;
  }

  return (hash >> str->len) % GLOBAL_SIZE;
#undef FNV_OFFSET_BASIS
#undef FNV_PRIME
}
