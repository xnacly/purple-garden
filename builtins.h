#pragma once

#include "common.h"
#include "mem.h"

// Represents the type signature for a builtin function
typedef Value *(*builtin_function)(const Value **args, size_t count,
                                   Allocator *alloc);

Value *builtin_print(const Value **arg, size_t count, Allocator *alloc);
Value *builtin_println(const Value **arg, size_t count, Allocator *alloc);
Value *builtin_len(const Value **arg, size_t count, Allocator *alloc);
Value *builtin_type(const Value **arg, size_t count, Allocator *alloc);
Value *builtin_assert(const Value **arg, size_t count, Allocator *alloc);
