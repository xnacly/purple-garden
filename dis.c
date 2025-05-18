#include "builtins.h"
#include "cc.h"
#include "common.h"
#include "strings.h"

void disassemble(const Vm *vm, const Ctx *ctx) {
  puts("; vim: filetype=asm");
  printf("; Vm {global=%u/%d, bytecode=%zu/%d}\n", vm->global_len, GLOBAL_SIZE,
         vm->bytecode_len, BYTECODE_SIZE);
  if (vm->global_len > 0) {
    printf("__globals:\n\t");
    for (size_t i = 0; i < vm->global_len; i++) {
      Value *v = vm->globals[i];
      Value_debug(v);
      printf("; {idx=%zu", i);
      if (v->type == V_STR) {
        printf(",hash=%zu", v->string.hash & GLOBAL_MASK);
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
          size_t location = ctx->function_hash_to_bytecode_index[j];
          if (location == i) {
            if (location != 0) {
              puts("");
            }
            printf("\n__0x%06zX[%04zX]: ", i, j);
            Str_debug(&ctx->function_hash_to_function_name[j]);
          }
        }
      }
      VM_OP op = vm->bytecode[i];
      size_t arg = vm->bytecode[i + 1];
#if DISASSEMBLE_INCLUDE_POSITIONS
      printf("\n\t; @0x%04zX/0x%04zX", i, i + 1);
#endif
      printf("\n\t");
      Str_debug(&OP_MAP[op]);

      // dont print the argument if its unused in the vm
      switch (op) {
      case OP_LEAVE:
        puts("");
      case OP_POP:
        break;
#if DISASSEMBLE_INCLUDE_POSITIONS
      case OP_JMP:
        printf(" 0x%04zX", arg);
        break;
#endif
      default:
        printf(" %zu", arg);
      }

      switch (op) {
      case OP_LOAD:
        printf(": ");
        Value_debug(vm->globals[arg]);
        break;
      case OP_LOADV:
        printf(": $");
        for (size_t j = 0; j < vm->global_len; j++) {
          Value *v = vm->globals[j];
          if (v->type == V_STR && (v->string.hash & GLOBAL_MASK) == arg) {
            Str_debug(&v->string);
          }
        }
        break;
      case OP_BUILTIN:
        printf(": <@%.*s>", (int)BUILTIN_NAME_MAP[arg].len,
               BUILTIN_NAME_MAP[arg].p);
        break;
      case OP_CALL: {
        for (size_t j = 0; j < MAX_BUILTIN_SIZE; j++) {
          size_t location = ctx->function_hash_to_bytecode_index[j];
          if (location == arg) {
            printf(": <");
            Str_debug(&ctx->function_hash_to_function_name[j]);
            printf(">");
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
