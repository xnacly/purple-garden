#include "common.h"
#include "lexer.h"
#include <stdio.h>
#include <stdlib.h>

String read_file_to_string(char *path) {
  if (path == NULL) {
    return STRING_EMPTY;
  }

  FILE *file = fopen(path, "rb");
  if (!file) {
    return STRING_EMPTY;
  }

  fseek(file, 0, SEEK_END);
  long length = ftell(file);
  rewind(file);

  if (length < 0) {
    fclose(file);
    return STRING_EMPTY;
  }

  char *buffer = malloc(length + 1);
  if (!buffer) {
    fclose(file);
    return STRING_EMPTY;
  }

  fread(buffer, 1, length, file);
  buffer[length] = '\0';

  fclose(file);
  return (String){.len = length, .p = buffer};
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

  free(input.p);

  return EXIT_SUCCESS;
}
