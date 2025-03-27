#include "../cc.h"
#include "../common.h"
#include "../lexer.h"
#include "../parser.h"
#include "../vm.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>

Vm setup(String str, byte *expected, size_t expected_size) {
  Lexer l = Lexer_new(str);
  Parser p = Parser_new(&l);
  Node ast = Parser_run(&p);
  Vm vm = cc(&ast);
  Node_destroy(&ast);
  return vm;
}

typedef struct {
  String input;
  size_t expected_size;
  byte *expected;
} Case;

#define BC(...)                                                                \
  (byte[]) { __VA_ARGS__ }
#define CASE(in, ex)                                                           \
  {                                                                            \
    .input = STRING(in), .expected = ex,                                       \
    .expected_size = sizeof(ex) / sizeof(byte)                                 \
  }

static Case cases[] = {
    CASE("3.1415", BC(OP_LOAD, 0)),
    CASE("true false", BC(OP_LOAD, 0, OP_LOAD, 1)),
    CASE("\"string\"", BC(OP_LOAD, 0)),
    CASE("ident", BC(OP_LOAD, 0, OP_VAR, 0)),
};

int main() {
  size_t passed = 0;
  size_t failed = 0;
  size_t len = sizeof(cases) / sizeof(Case);
  for (size_t i = 0; i < len; i++) {
    Case c = cases[i];
    printf("[Case %zu/%zu] '%s' \n", i + 1, len, c.input.p);
    Vm raw = setup(c.input, c.expected, c.expected_size);
    Vm *vm = &raw;

    bool error = false;
    if (c.expected_size != vm->bytecode_len) {
      printf("\tlenght not equal: wanted %zu, got %zu\n", c.expected_size,
             vm->bytecode_len);
      error = true;
    } else {
      for (size_t j = 0; j < vm->bytecode_len; j += 2) {
        size_t expected_op = c.expected[j];
        size_t got_op = vm->bytecode[j];

        size_t expected_arg = c.expected[j + 1];
        size_t got_arg = vm->bytecode[j + 1];

        if (expected_op != got_op) {
#if DEBUG
          printf("\tbad operator: want=%s got=%s\n", OP_MAP[expected_op].p,
                 OP_MAP[got_op].p);
#else
          printf("\n\tbad operator: want=%zu got=%zu\n", expected_op, got_op);
#endif

          error = true;
        }
        if (expected_arg != got_arg) {
          printf("\tbad arg: want=%zu got=%zu\n", expected_arg, got_arg);
          error = true;
        }
      }
    }
    Vm_run(vm);

    if (error) {
      failed++;
      puts("~> failed!");
    } else {
      passed++;
      puts("~> passed!");
    }
    Vm_destroy(raw);
  }

  printf("%zu of %zu passed, %zu failed\n", passed, len, failed);

  return failed == 0 ? EXIT_SUCCESS : EXIT_FAILURE;
}
