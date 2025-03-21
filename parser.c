#include "parser.h"
#include "common.h"
#include "lexer.h"

Parser Parser_new(List list) {
  return (Parser){
      .input = list,
      .pos = 0,
  };
}

static boolean at_end(Parser *p) { return p->pos >= p->input.len; }
static Token cur(Parser *p) { return List_get(&p->input, p->pos); }
static void advance(Parser *p) {
  if (!at_end(p)) {
    p->pos++;
  }
}
// static void consume(Parser *p, TokenType tt) {
//   ASSERT(cur(p)->type == tt, "Unexpected token")
//   advance(p);
// }

Node Parser_parse(Parser *p) {
  while (!at_end(p)) {
    puts(String_to(&TOKEN_TYPE_MAP[cur(p).type]));
    advance(p);
  }
  return (Node){};
}
