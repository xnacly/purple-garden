#ifndef LIST_H
#define LIST_H

#include "lexer.h"
#include <stdlib.h>

typedef struct List {
  // list of elements
  Token *elements;
  // count of elements currenlty in list
  size_t len;
  // maxium size of Slice
  size_t cap;
} List;

// creates and returns a new slice, if initial_size is less than
// SLICE_MIN_SIZE, initial_size gets set to SLICE_MIN_SIZE.
List *List_new(size_t initial_size);

// inserts element at the given index, if s.len would be bigger than s.cap
// after insertion, doubles the size of the underlying array
void List_append(List *s, Token element);

// returns the given element if 0 <= index < s.len
Token List_get(List *s, size_t index);

// frees the allocated memory region for the given Slice, sets the
// pointer to point to NULL
void List_free(List *s);

#endif
