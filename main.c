#include "common.h"
#include "lexer.h"
#include <stdio.h>
#include <stdlib.h>

String read_file_to_string(char *path) {
  if (path == NULL) {
    goto cleanup;
  }

cleanup:
  return STRING_EMPTY;
}

int main(int argc, char **args) {
  if (argc < 2) {
    fprintf(stderr, "Wanted a filename as an argument");
    return EXIT_FAILURE;
  }

  char *filename = args[1];
  String input = read_file_to_string(filename);

  Lexer l = Lexer_new(input);
  while (true) {
    Token t = Lexer_next(&l);
    if (t.type == T_EOF) {
      break;
    }
    Token_debug(&t);
    Token_destroy(&t);
  }

  return EXIT_SUCCESS;
}
