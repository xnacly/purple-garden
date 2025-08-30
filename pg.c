#include "pg.h"
#include "cc.h"
#include "common.h"
#include "io.h"
#include "lexer.h"
#include "mem.h"
#include "parser.h"
#include "vm.h"

Pg pg_init(Vm_Config *conf) {
  Allocator *a = bump_init(MIN_MEM, conf->max_memory);
  return (Pg){
      .__alloc = a,
      .__vm = Vm_new(*conf, a, NULL),
      .__conf = conf,
  };
}

uint8_t pg_exec_Str(Pg *pg, Str input) {
  Lexer lexer = Lexer_new(input);
  Parser parser = Parser_new(pg->__alloc, &lexer);
  Ctx ctx = cc(&pg->__vm, pg->__alloc, &parser);

  if (pg->__conf->disable_gc) {
    pg->__vm.alloc = pg->__alloc;
  } else {
    pg->__vm.alloc = xcgc_init(&pg->__vm, GC_MIN_HEAP, pg->__conf->max_memory);
  }

  return Vm_run(&pg->__vm);
}

uint8_t pg_exec_file(Pg *pg, const char *filename) {
  Str input = IO_read_file_to_string(filename);
  return pg_exec_Str(pg, input);
}

void pg_destroy(Pg *pg) {
  Vm_destroy(&pg->__vm);
  // Since the gc was disabled and the allocator thus wasnt destroyed by the vm,
  // we will do it here.
  if (!pg->__conf->disable_gc) {
    CALL(pg->__alloc, destroy);
  }
}
