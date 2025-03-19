#include "common.h"
#include "lexer.h"
#include <stdio.h>
#include <stdlib.h>

int main(void) {
  String s = STRING("((()))");
  Lexer l = Lexer_new(s);
  while (true) {
    Token t = Lexer_next(&l);
    if (t.type == T_EOF) {
      break;
    }
    puts(String_to(&TOKEN_TYPE_MAP[t.type]));
  }

  return EXIT_SUCCESS;
}
