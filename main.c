#include <stdlib.h>

#include "cc.h"
#include "common.h"
#include "io.h"
#include "lexer.h"
#include "parser.h"
#include "vm.h"

int main(int argc, char **args) {
  ASSERT(argc >= 2, "Wanted a filename as an argument, not enough arguments")

  char *filename = args[1];
  String input = IO_read_file_to_string(filename);

  Lexer l = Lexer_new(input);
  Parser p = Parser_new(&l);
  Node ast = Parser_run(&p);
  Vm vm = cc(&ast);
  Vm_run(&vm);
  Node_destroy(&ast);
  Vm_destroy(vm);
  free(input.p);

  return EXIT_SUCCESS;
}
