#include "vm.h"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct {
  uint8_t opcode;
  size_t compiled;
  size_t executed;
} OpStat;

static int sort(const void *a, const void *b) {
  size_t ta = ((const OpStat *)a)->compiled + ((const OpStat *)a)->executed;
  size_t tb = ((const OpStat *)b)->compiled + ((const OpStat *)b)->executed;
  return (tb > ta) - (tb < ta);
}

void bytecode_stats(const Vm *vm) {
  size_t compiled[256] = {0};
  size_t executed[256] = {0};
  size_t total_compiled = 0;
  size_t total_executed = 0;

  for (uint64_t i = 0; i < vm->bytecode_len; i += 2) {
    uint8_t op = vm->bytecode[i];
    compiled[op]++;
    total_compiled++;
  }

#if DEBUG
  for (int i = 0; i < 256; i++) {
    executed[i] = vm->instruction_counter[i];
    total_executed += executed[i];
  }
#endif

  OpStat ops[256] = {0};
  int op_len = 0;

  for (int i = 0; i < 256; i++) {
    if (compiled[i]
#if DEBUG
        || executed[i]
#endif
    ) {
      ops[op_len++] = (OpStat){.opcode = i,
                               .compiled = compiled[i],
#if DEBUG
                               .executed = executed[i]
#else
                               .executed = 0
#endif
      };
    }
  }

  qsort(ops, op_len, sizeof(OpStat), sort);

  printf("| %-10s | %-24s | %-24s |\n", "Opcode", "Compiled %", "Executed %");
  printf("| ---------- | ------------------------ | ------------------------ | "
         "\n");

  for (int i = 0; i < op_len; i++) {
    uint8_t op = ops[i].opcode;
    double comp_pct =
        total_compiled ? (100.0 * ops[i].compiled) / total_compiled : 0.0;
    double exec_pct =
#if DEBUG
        total_executed ? (100.0 * ops[i].executed) / total_executed : 0.0;
#else
        0.0;
#endif

    printf("| %-10.*s | %-15zu (%05.2f%%) | %-15zu (%05.2f%%) |\n",
           (int)OP_MAP[op].len, OP_MAP[op].p, ops[i].compiled, comp_pct,
           ops[i].executed, exec_pct);
  }
  printf("| ========== | ======================== | ======================== | "
         "\n");
  printf("| %-10s | %-15zu (%05.2f%%) | %-15zu (%05.2f%%) |\n", "::<>",
         total_compiled, 99.99, total_executed, 99.99);
}
