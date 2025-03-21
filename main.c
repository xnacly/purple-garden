#include <stdlib.h>

#include "common.h"
#include "io.h"
#include "lexer.h"

int main(int argc, char **args) {
  ASSERT(argc >= 2, "Wanted a filename as an argument, not enough arguments")

  char *filename = args[1];
  String input = IO_read_file_to_string(filename);

  Lexer l = Lexer_new(input);
  while (true) {
    Token t = Lexer_next(&l);
    if (t.type == T_EOF) {
      break;
    }
    Token_debug(&t);
    Token_destroy(&t);
  }

  free(input.p);

  return EXIT_SUCCESS;
}
