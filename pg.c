#include "pg.h"
#include "cc.h"
#include "common.h"
#include "gc.h"
#include "io.h"
#include "lexer.h"
#include "mem.h"
#include "parser.h"
#include "vm.h"

Pg pg_init(Vm_Config *conf) {
  Allocator *a = bump_init(MIN_MEM, conf->max_memory);
  Gc *gc = a->request(a->ctx, sizeof(Gc));
  *gc = gc_init(0);
  return (Pg){
      .__alloc = a,
      .__vm = Vm_new(*conf, a, gc),
      .__conf = conf,
  };
}

uint8_t pg_exec_Str(Pg *pg, Str input) {
  ASSERT(pg->__alloc != NULL,
         "pg: missing allocator, context not initialized?");
  ASSERT(pg->__vm.gc != NULL, "pg: context not initialized");
  Lexer lexer = Lexer_new(input);
  Parser parser = Parser_new(pg->__alloc, &lexer);
  Ctx ctx = cc(&pg->__vm, pg->__alloc, &parser);
  return Vm_run(&pg->__vm);
}

uint8_t pg_exec_file(Pg *pg, const char *filename) {
  Str input = IO_read_file_to_string(filename);
  return pg_exec_Str(pg, input);
}

void pg_destroy(Pg *pg) {
  CALL(pg->__alloc, destroy);
  CALL(pg->__vm.gc->old, destroy);
  free(pg->__vm.gc->old);
  CALL(pg->__vm.gc->new, destroy);
  free(pg->__vm.gc->new);
  free(pg->__alloc);
}
