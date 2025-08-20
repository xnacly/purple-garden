#include "pg.h"
#include "cc.h"
#include "io.h"
#include "lexer.h"
#include "mem.h"
#include "parser.h"
#include "vm.h"

#define MIN_MEMORY 1 * 1024

Pg pg_init(Vm_Config *conf) {
  if (conf->max_memory < MIN_MEMORY) {
    conf->max_memory = MIN_MEMORY;
  }
  Allocator *a = bump_init(conf->max_memory / 2);
  return (Pg){
      .__alloc = a,
      .__vm = Vm_new(*conf, a, NULL),
      .__conf = conf,
  };
}

// TODO: I have to make this better, maybe reattach lexer to the parser to not
// use as much memory
#define MAX_TOKENS 100000
#define MAX_NODES 100000

uint8_t pg_exec_file(Pg *pg, const char *filename) {
  Str input = IO_read_file_to_string(filename);
  Lexer l = Lexer_new(input);
  Token **tokens = CALL(pg->__alloc, request, MAX_TOKENS * sizeof(Token *));
  size_t count = Lexer_all(&l, pg->__alloc, tokens);
  Parser p = Parser_new(pg->__alloc, tokens);
  Node **nodes = CALL(pg->__alloc, request, MAX_NODES);
  size_t node_count = Parser_all(nodes, &p, MAX_NODES);
  Ctx ctx = cc(&pg->__vm, pg->__alloc, nodes, node_count);

  if (pg->__conf->disable_gc) {
    pg->__vm.alloc = bump_init(pg->__conf->max_memory / 2);
  } else {
    pg->__vm.alloc = xcgc_init(pg->__conf->max_memory / 2, &pg->__vm);
  }

  return Vm_run(&pg->__vm);
}

void pg_destroy(Pg *pg) {
  Vm_destroy(&pg->__vm);
  CALL(pg->__alloc, destroy);
}
