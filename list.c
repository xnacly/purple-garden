
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "common.h"
#include "list.h"

#define SLICE_MIN_SIZE 8

List *List_new(size_t initial_size) {
  List *s = malloc(sizeof(List));
  ASSERT(s != NULL, "failed to allocate a list")
  initial_size = initial_size < SLICE_MIN_SIZE ? SLICE_MIN_SIZE : initial_size;
  s->elements = malloc(initial_size * sizeof(void *));
  ASSERT(s->elements != NULL,
         "failed to allocate the underlying array for a list")
  s->cap = initial_size;
  s->len = 0;
  return s;
}

// duplicates the cap of the slice and the allocated space of the underlying
// array
static void grow_slice(List *s) {
  Token *t = malloc(s->cap * 2 * sizeof(Token));
  ASSERT(t != NULL, "failed to grow the slice")
  for (size_t i = 0; i < s->len; i++) {
    t[i] = s->elements[i];
  }
  free(s->elements);
  s->elements = t;
  s->cap *= 2;
}

void List_append(List *s, Token t) {
  // if we append and the new len is bigger or equal to the size we double the
  // slice, therefore we make a good tradeoff between times we need to grow
  // the slice and the amount of memory we take up
  if (s->len + 1 > s->cap) {
    grow_slice(s);
  }

  s->elements[s->len] = t;
  s->len++;
}

Token List_get(List *s, size_t index) {
  ASSERT(!(index >= s->cap || index >= s->len), "index out of bounds")
  return s->elements[index];
}

void List_free(List *s) {
  free(s->elements);
  free(s);
  s = NULL;
}
