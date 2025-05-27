#include "cc.h"
#include "vm.h"
#include <stdint.h>
#include <stdlib.h>

typedef struct {
  uint8_t opcode;
  size_t count;
} OpCount;

static int sort(const void *a, const void *b) {
  size_t ca = ((const OpCount *)a)->count;
  size_t cb = ((const OpCount *)b)->count;
  return (cb > ca) - (cb < ca);
}

void stats(const Vm *vm) {
  size_t counter[256] = {0};
  for (uint64_t i = 0; i < vm->bytecode_len; i += 2) {
    counter[vm->bytecode[i]]++;
  }

  OpCount ops[256] = {0};
  int op_len = 0;
  for (int i = 0; i < 256; i++) {
    if (counter[i]) {
      ops[op_len++] = (OpCount){.opcode = i, .count = counter[i]};
    }
  }

  qsort(ops, op_len, sizeof(OpCount), sort);

  double total = (double)vm->bytecode_len / 2;
  for (int i = 0; i < op_len; i++) {
    uint8_t op = ops[i].opcode;
    size_t count = ops[i].count;
    double percent = 100.0 * count / total;
    printf("\t%-10.*s: %-10zu (%05.2f%%)\n", (int)OP_MAP[op].len, OP_MAP[op].p,
           count, percent);
  }
}
