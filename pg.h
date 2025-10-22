#pragma once

/*
 * pg.h defines the public facing api of purple garden which can be used to
 * embed purple garden
 */

#include <stdint.h>

#include "vm.h"

#define PG_API __attribute__((visibility("default")))

// Pg holds all dependencies necessary for creating a purple garden context
typedef struct Pg {
  Allocator *__alloc;
  Vm __vm;
  Vm_Config *__conf;
} Pg;

// Pointer all registered functions have to adhere to
typedef void (*builtin_function)(Vm *vm);

PG_API Pg pg_init(Vm_Config *conf);
PG_API uint8_t pg_exec_file(Pg *pg, const char *filename);
PG_API uint8_t pg_exec_Str(Pg *pg, Str input);
PG_API void pg_destroy(Pg *pg);

/* C and purple garden interop */
#define VALUE_FROM(CVAL)
#define VALUE_TO(PGVAL)
