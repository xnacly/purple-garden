#include <stdlib.h>

#include "common.h"
#include "io.h"
#include "lexer.h"
#include "list.h"

int main(int argc, char **args) {
  ASSERT(argc >= 2, "Wanted a filename as an argument, not enough arguments")

  char *filename = args[1];
  String input = IO_read_file_to_string(filename);

  Lexer l = Lexer_new(input);
  List *tokens = List_new(0);
  while (true) {
    Token t = Lexer_next(&l);
    List_append(tokens, t);
    if (t.type == T_EOF) {
      break;
    }
  }

  for (size_t i = 0; i < tokens->len; i++) {
    Token t = List_get(tokens, i);
    Token_destroy(&t);
  }
  List_free(tokens);

  free(input.p);

  return EXIT_SUCCESS;
}
