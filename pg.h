#pragma once

/* pg.h defines the public facing api of purple garden which can be used to
 * embed purple garden
 */

#include <stdint.h>

#include "std/std.h"
#include "vm.h"

#define PG_API __attribute__((visibility("default")))

typedef struct Pg {
  Allocator *__alloc;
  Vm __vm;
  Vm_Config *__conf;
} Pg;

typedef void (*builtin_function)(Vm *vm);

PG_API Pg pg_init(Vm_Config *conf);
PG_API uint8_t pg_exec_file(Pg *pg, const char *filename);
PG_API uint8_t pg_exec_Str(Pg *pg, Str input);
PG_API void pg_destroy(Pg *pg);
