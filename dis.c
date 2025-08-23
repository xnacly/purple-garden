#include "cc.h"
#include "common.h"
#include "strings.h"
#include "vm.h"

void disassemble(const Vm *vm, const Ctx *ctx) {
  if (vm->global_len > 0) {
    printf("__globals:\n\t");
    for (size_t i = 0; i < vm->global_len; i++) {
      Value *v = vm->globals[i];
      Value_debug(v);
      printf("; {idx=%zu", i);
      if (v->type == V_STR) {
        printf(",hash=%zu", v->string.hash & GLOBAL_SIZE_MASK);
      }
      printf("}\n\t");
    }
  }

  bool ctx_available = ctx != NULL;

  if (vm->bytecode_len > 0) {
    printf("\n__entry:");
    for (size_t i = 0; i < vm->bytecode_len; i += 2) {
      if (ctx_available) {
        for (size_t j = 0; j < MAX_BUILTIN_SIZE; j++) {
          CtxFunction func = ctx->hash_to_function[j];
          if (func.bytecode_index == i && func.name != NULL) {
            if (func.bytecode_index != 0) {
              puts("");
            }
            printf("\n; %.*s::{args=%zu,size=%zu}\n__0x%06zX[%04zX]: ",
                   (int)func.name->len, func.name->p, func.argument_count,
                   func.size, i, j);
          }
        }
      }
      VM_OP op = vm->bytecode[i];
      size_t arg = vm->bytecode[i + 1];
      printf("\n\t");
#if DISASSEMBLE_INCLUDE_POSITIONS
      printf("[%04zu|%04zu]\t", i, i + 1);
#endif
      Str_debug(&OP_MAP[op]);

      // dont print the argument if its unused in the vm
      switch (op) {
      case OP_ARGS:
        printf(" %zu ; count=%zu,offset=%zu", arg, DECODE_ARG_COUNT(arg),
               DECODE_ARG_OFFSET(arg));
        break;
      case OP_LEAVE:
        puts("");
      case OP_ASSERT:
        break;
#if DISASSEMBLE_INCLUDE_POSITIONS
      case OP_JMP:
      case OP_JMPF:
        printf(" %04zu", arg);
        break;
#endif
      default:
        printf(" %zu", arg);
      }

      switch (op) {
      case OP_LOADG:
        printf("; ");
        Value_debug(vm->globals[arg]);
        break;
      case OP_CALL: {
        for (size_t j = 0; j < MAX_BUILTIN_SIZE; j++) {
          CtxFunction func = ctx->hash_to_function[j];
          if (func.bytecode_index == arg && func.name != NULL) {
            printf("; <");
            Str_debug(func.name);
            printf("> $%zu", func.argument_count);
          }
        }
        break;
      }
      default:
        break;
      }
    }
  }
}
