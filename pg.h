/* pg.h defines the public facing api of purple garden which can be used to
 * embed purple garden
 */
#pragma once

#include <stdint.h>

#include "strings.h"
#include "vm.h"

typedef struct Pg Pg;

typedef void (*builtin_function)(Vm *vm);

#define PG_REGISTER_BUILTIN(PG, NAME, FN)                                      \
  Vm_register_builtin(&pg->__vm, fn, STRING(name))

Pg pg_init(uint64_t max_memory);
uint8_t pg_exec_file(Pg *pg, const char *filename);
void pg_destroy(Pg *pg);
