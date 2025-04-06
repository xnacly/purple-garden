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
  size_t expected_size;
  byte *expected;
  Value expected_r0;
} Case;

#define BC(...)                                                                \
  (byte[]) { __VA_ARGS__ }
#define VAL(...)                                                               \
  (Value) { __VA_ARGS__ }

#define CASE(in, ex, r0)                                                       \
  {                                                                            \
      .input = STRING(#in "\0"),                                               \
      .expected = ex,                                                          \
      .expected_size = sizeof(ex) / sizeof(byte),                              \
      .expected_r0 = r0,                                                       \
  }

int main() {
  Case cases[] = {
    // atoms:
    CASE(3.1415, BC(OP_LOAD, 0), VAL(.type = V_NUM, .number = 3.1415)),
    CASE(.1415, BC(OP_LOAD, 0), VAL(.type = V_NUM, .number = 0.1415)),
    CASE("string", BC(OP_LOAD, 0),
         VAL(.type = V_STRING, .string = STRING("string"))),
    // TODO: this is for future me to implement
    // CASE("escaped string\"", BC(OP_LOAD, 0), VAL(.type = V_STRING, .string
    // = STRING("escaped string\""))),
    CASE(true false, BC(OP_LOAD, 0, OP_LOAD, 1), VAL(.type = V_FALSE)),
    CASE("hello", BC(OP_LOAD, 0),
         VAL(.type = V_STRING, .string = STRING("hello"))),

    // INFO: infinity comparison case:
    // https://github.com/xNaCly/purple-garden/issues/1
    // CASE("1.7976931348623157e+309", BC(OP_LOAD, 0),
    //      VAL(.type = V_NUM, .number = 1.7976931348623157E+309)),

    // math:
    CASE((+2 2), BC(OP_LOAD, 0, OP_STORE, 1, OP_LOAD, 1, OP_ADD, 1),
         VAL(.type = V_NUM, .number = 4)),
    CASE((-5 3), BC(OP_LOAD, 0, OP_STORE, 1, OP_LOAD, 1, OP_SUB, 1),
         VAL(.type = V_NUM, .number = 2)),
    CASE((*3 4), BC(OP_LOAD, 0, OP_STORE, 1, OP_LOAD, 1, OP_MUL, 1),
         VAL(.type = V_NUM, .number = 12)),
    CASE((/ 6 2), BC(OP_LOAD, 0, OP_STORE, 1, OP_LOAD, 1, OP_DIV, 1),
         VAL(.type = V_NUM, .number = 3)),

    // builtins:
    CASE((@len "hello"), BC(OP_LOAD, 0, OP_BUILTIN, BUILTIN_LEN),
         VAL(.type = V_NUM, .number = 5)),
    CASE((@len ""), BC(OP_LOAD, 0, OP_BUILTIN, BUILTIN_LEN),
         VAL(.type = V_NUM, .number = 0)),
    CASE((@len "a"), BC(OP_LOAD, 0, OP_BUILTIN, BUILTIN_LEN),
         VAL(.type = V_NUM, .number = 1)),
  };
  size_t passed = 0;
  size_t failed = 0;
  size_t len = sizeof(cases) / sizeof(Case);
  for (size_t i = 0; i < len; i++) {
    Case c = cases[i];
    Lexer l = Lexer_new(c.input);
    Parser p = Parser_new(&l);
    Node ast = Parser_run(&p);
    Vm raw = cc(&ast);
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
          printf("\tbad operator: want=%s got=%s\n", OP_MAP[expected_op].p,
                 OP_MAP[got_op].p);
          error = true;
        }
        if (expected_arg != got_arg) {
          printf("\tbad arg: want=%zu got=%zu\n", expected_arg, got_arg);
          error = true;
        }
      }
    }
    Vm_run(vm);
    if (!Value_cmp(vm->registers[0], c.expected_r0)) {
      printf("\n\tbad value at r0: want=%s got=%s",
             VALUE_TYPE_MAP[c.expected_r0.type].p,
             VALUE_TYPE_MAP[vm->registers[0].type].p);
      printf("\n\twant=");
      Value_debug(&c.expected_r0);
      printf("\n\tgot=");
      Value_debug(&vm->registers[0]);
      puts("");
      error = true;
    }
    Node_destroy(&ast);

    if (error) {
      failed++;
      printf("[-][FAIL][Case %zu/%zu] in=`%s` \n", i + 1, len, c.input.p);
    } else {
      passed++;
      printf("[+][PASS][Case %zu/%zu] in=`%s` \n", i + 1, len, c.input.p);
    }
    Vm_destroy(raw);
  }

  printf("%zu of %zu passed, %zu failed\n", passed, len, failed);

  return failed == 0 ? EXIT_SUCCESS : EXIT_FAILURE;
}
