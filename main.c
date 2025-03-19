#include "common.h"
#include "lexer.h"
#include <stdlib.h>

#define $(a) STRING(#a)

static const String input = $(("hello world"
                               "I'm like hey whatsuphello"i 3.1415 ident));

int main(void) {
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
