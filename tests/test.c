#include "../builtins.h"
#include "../cc.h"
#include "../common.h"
#include "../lexer.h"
#include "../parser.h"
#include "../vm.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct {
  Str input;
  Value expected_r0;
} Case;

#define VAL(...)                                                               \
  (Value) { __VA_ARGS__ }

#define CASE(in, r0)                                                           \
  {                                                                            \
      .input = STRING(#in "\0"),                                               \
      .expected_r0 = r0,                                                       \
  }

int main() {
  Case cases[] = {
    // atoms:
    CASE(3.1415, VAL(.type = V_NUM, .number = 3.1415)),
    CASE(.1415, VAL(.type = V_NUM, .number = 0.1415)),
    CASE("string", VAL(.type = V_STR, .string = STRING("string"))),
    // TODO: this is for future me to implement
    // CASE("escaped string\"", BC(OP_LOAD, 0), VAL(.type = V_STRING, .string
    // = STRING("escaped string\""))),
    CASE(true false, VAL(.type = V_FALSE)),
    // checking if boolean interning works
    CASE(true false true false, VAL(.type = V_FALSE)),
    CASE("hello", VAL(.type = V_STR, .string = STRING("hello"))),

    // infinity comparison case:
    // https://github.com/xNaCly/purple-garden/issues/1
    // CASE("1.7976931348623157e+309", BC(OP_LOAD, 0),
    //      VAL(.type = V_NUM, .number = 1.7976931348623157E+309)),

    // math:
    CASE((+2 2), VAL(.type = V_NUM, .number = 4)),
    CASE((-5 3), VAL(.type = V_NUM, .number = 2)),
    CASE((*3 4), VAL(.type = V_NUM, .number = 12)),
    CASE((/ 6 2), VAL(.type = V_NUM, .number = 3)),
    CASE((+1(-2 1)), VAL(.type = V_NUM, .number = 2)),

    // builtins:
    CASE((@len "hello"), VAL(.type = V_NUM, .number = 5)),
    // checking if string interning works
    CASE((@len "hello")(@len "hello"), VAL(.type = V_NUM, .number = 5)),
    CASE((@len ""), VAL(.type = V_NUM, .number = 0)),
    CASE((@len "a"), VAL(.type = V_NUM, .number = 1)),

    // variables
    CASE((@let name "user"), VAL(.type = V_STR, .string = STRING("name"))),
    CASE((@let name "user")name, VAL(.type = V_STR, .string = STRING("user"))),
    CASE((@let age 25)age, VAL(.type = V_NUM, .number = 25)),

    // functions
    CASE((@function ret[arg] arg)(ret 25), VAL(.type = V_NUM, .number = 25)),
    CASE((@function add25[arg](+arg 25))(add25 25),
         VAL(.type = V_NUM, .number = 50)),
  };
  size_t passed = 0;
  size_t failed = 0;
  size_t len = sizeof(cases) / sizeof(Case);
  for (size_t i = 0; i < len; i++) {
    Case c = cases[i];
    Allocator alloc = {
        .init = bump_init,
        .request = bump_request,
        .destroy = bump_destroy,
        .reset = bump_reset,
    };

    size_t min_size = (
                          // size for globals
                          (MIN_MEM * sizeof(Value))
                          // size for bytecode
                          + MIN_MEM
                          // size for nodes
                          + (MIN_MEM * sizeof(Node))) *
                      2;

    Lexer l = Lexer_new(c.input);
    alloc.ctx = alloc.init(min_size);
    Token **tokens = alloc.request(alloc.ctx, MIN_MEM * sizeof(Token));
    Lexer_all(&l, &alloc, tokens);
    Parser p = Parser_new(&alloc, tokens);
    size_t node_count = MIN_MEM * sizeof(Node *) / 4;
    Node **nodes = alloc.request(alloc.ctx, node_count);
    node_count = Parser_all(nodes, &p, node_count);
    CompileOutput out = cc(&alloc, nodes, node_count);
    Vm *vm = &out.vm;

    bool error = false;
    Vm_run(vm, &alloc);
    if (!Value_cmp(vm->registers[0], c.expected_r0)) {
      printf("\n\tbad value at r0: want=%s got=%s",
             VALUE_TYPE_MAP[c.expected_r0.type].p,
             VALUE_TYPE_MAP[vm->registers[0].type].p);
      printf("\n\twant=");
      Value_debug(&c.expected_r0);
      printf("\n\tgot=");
      Value_debug(&vm->registers[0]);
      puts("");
      disassemble(vm, &out.ctx);
      error = true;
    }
    alloc.destroy(alloc.ctx);

    if (error) {
      failed++;
      printf("[-][FAIL][Case %zu/%zu] in=`%s` \n", i + 1, len, c.input.p);
    } else {
      passed++;
      printf("[+][PASS][Case %zu/%zu] in=`%s` \n", i + 1, len, c.input.p);
    }
    Vm_destroy(vm);
  }

  printf("%zu of %zu passed, %zu failed\n", passed, len, failed);

  return failed == 0 ? EXIT_SUCCESS : EXIT_FAILURE;
}
