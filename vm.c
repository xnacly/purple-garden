#include <stdint.h>

#include "adts.h"
#include "common.h"
#include "mem.h"
#include "strings.h"
#include "vm.h"

Str OP_MAP[256] = {
    [OP_STORE] = STRING("STORE"),   [OP_LOAD] = STRING("LOAD"),
    [OP_ADD] = STRING("ADD"),       [OP_SUB] = STRING("SUB"),
    [OP_MUL] = STRING("MUL"),       [OP_DIV] = STRING("DIV"),
    [OP_EQ] = STRING("EQ"),         [OP_LT] = STRING("LT"),
    [OP_GT] = STRING("GT"),         [OP_VAR] = STRING("VAR"),
    [OP_LOADV] = STRING("LOADV"),   [OP_ARGS] = STRING("ARGS"),
    [OP_SYS] = STRING("BUILTIN"),   [OP_LEAVE] = STRING("LEAVE"),
    [OP_CALL] = STRING("CALL"),     [OP_JMP] = STRING("JMP"),
    [OP_LOADG] = STRING("LOADG"),   [OP_JMPF] = STRING("JMPF"),
    [OP_APPEND] = STRING("APPEND"), [OP_NEW] = STRING("NEW"),
    [OP_SIZE] = STRING("SIZE"),     [OP_IDX] = STRING("IDX"),
};

static builtin_function BUILTIN_MAP[MAX_BUILTIN_SIZE] = {0};

Vm Vm_new(Vm_Config conf, Allocator *static_alloc, Allocator *alloc) {
  Vm vm = {0};
  vm.alloc = alloc;
  vm.config = conf;

  vm.builtins = BUILTIN_MAP;

  vm.globals = CALL(static_alloc, request, (sizeof(Value) * GLOBAL_SIZE));
  vm.globals[GLOBAL_FALSE] = *INTERNED_FALSE;
  vm.globals[GLOBAL_TRUE] = *INTERNED_TRUE;
  vm.globals[GLOBAL_NONE] = *INTERNED_NONE;
  vm.global_len = 3;

  return vm;
}

// FrameFreeList works by caching a list of Frames so we don't have to interact
// with the heap for the first N Frames, but instead use the preallocated
// frames. On entering a scope, pop a frame and use it, on leaving a scope,
// clear the frame and return it.
typedef struct {
  Frame *head;
  Allocator *alloc;
} FrameFreeList;

void freelist_preallocate(FrameFreeList *fl) {
  for (int i = 0; i < PREALLOCATE_FREELIST_SIZE; i++) {
    Frame *frame = CALL(fl->alloc, request, sizeof(Frame));
    *frame = (Frame){0};
    frame->variable_table =
        CALL(fl->alloc, request, sizeof(Value) * VARIABLE_TABLE_SIZE);
    memset(frame->variable_table, 0, sizeof(Value) * VARIABLE_TABLE_SIZE);
    frame->prev = fl->head;
    fl->head = frame;
  }
}

void freelist_push(FrameFreeList *fl, Frame *frame) {
  frame->prev = fl->head;
  frame->return_to_bytecode = 0;
  fl->head = frame;
}

Frame *freelist_pop(FrameFreeList *fl) {
  if (!fl->head) {
    Frame *f = CALL(fl->alloc, request, sizeof(Frame));
    f->variable_table =
        CALL(fl->alloc, request, sizeof(Value) * VARIABLE_TABLE_SIZE);
    memset(f->variable_table, 0, sizeof(Value) * VARIABLE_TABLE_SIZE);
    return f;
  }
  Frame *f = fl->head;
  fl->head = f->prev;
  f->prev = NULL;
  return f;
}

int Vm_run(Vm *vm) {
  FrameFreeList *fl = &(FrameFreeList){.alloc = vm->alloc};
#if PREALLOCATE_FREELIST_SIZE
  freelist_preallocate(fl);
#endif
  vm->arg_count = 1;
  vm->frame = freelist_pop(fl);
  while (vm->pc < vm->bytecode_len) {
    VM_OP op = vm->bytecode[vm->pc];
    uint32_t arg = vm->bytecode[vm->pc + 1];

#define PRINT_REGISTERS 2
#if DEBUG
    vm->instruction_counter[op]++;
    Str *str = &OP_MAP[op];
    printf("[VM][%06zu|%05zu] %.*s%*.s=%06u", vm->pc, vm->pc + 1, (int)str->len,
           str->p, 10 - (int)str->len, " ", arg);
#if PRINT_REGISTERS
    printf("{ ");
    for (size_t i = 0; i < PRINT_REGISTERS; i++) {
      Value_debug(&vm->registers[i]);
      printf(" ");
    }
    printf("} ");
#endif
    puts("");
#endif

    switch (op) {
    case OP_SIZE:
      vm->size_hint = arg;
      break;
    case OP_NEW: {
      Value v = (Value){0};
      switch ((VM_New)arg) {
      case VM_NEW_ARRAY:
        v.type = V_ARRAY;
        if (vm->size_hint != 0) {
          LIST_Value *lv = CALL(vm->alloc, request, sizeof(LIST_Value));
          *lv = LIST_new(Value);
          v.array = lv;
        } else {
          LIST_Value *lv = CALL(vm->alloc, request, sizeof(LIST_Value));
          *lv = (LIST_Value){
              .len = 0,
          };
          v.array = lv;
        }
        break;
      case VM_NEW_OBJ: {
        v.type = V_OBJ;
        Map *m = CALL(vm->alloc, request, sizeof(Map));
        if (vm->size_hint != 0) {
          *m = Map_new(vm->size_hint, vm->alloc);
        } else {
          *m = (Map){0};
        }
        v.obj = m;
        break;
      }
      default:
        ASSERT(0, "OP_NEW unimplemented");
        break;
      }
      vm->registers[0] = v;
      vm->size_hint = 0;
      break;
    }
    case OP_APPEND:
      LIST_append(vm->registers[arg].array, vm->alloc, vm->registers[0]);
      break;
    case OP_LOADG:
      vm->registers[0] = vm->globals[arg];
      break;
    case OP_LOAD:
      vm->registers[0] = vm->registers[arg];
      break;
    case OP_LOADV: {
      // bounds checking and checking for variable validity is performed at
      // compile time, but we still have to check if the variable is available
      // in the current scope...
      Value v = vm->frame->variable_table[arg];
      // TODO: this doesnt work, we need a different way of checking access,
      // this disallows putting (@None) in the variable table if (v.type ==
      // V_NONE) {
      //   VM_ERR("Undefined variable with hash %i", arg);
      // }
      vm->registers[0] = v;
      break;
    }
    case OP_STORE:
      vm->registers[arg] = vm->registers[0];
      break;
    case OP_VAR:
      vm->frame->variable_table[arg] = vm->registers[0];
      break;
    case OP_ADD: {
      Value *lhs = &vm->registers[0];
      Value *rhs = &vm->registers[arg];

      if (lhs->type == V_STR && rhs->type == V_STR) {
        Str *s = CALL(vm->alloc, request, sizeof(Str));
        *s = Str_concat(rhs->string, lhs->string, vm->alloc);
        vm->registers[0] = (Value){.type = V_STR, .string = s};
        break;
      }

      int lhs_is_double = lhs->type == V_DOUBLE;
      int rhs_is_double = rhs->type == V_DOUBLE;

      if (lhs_is_double | rhs_is_double) {
        double a = lhs_is_double ? lhs->floating : (double)lhs->integer;
        double b = rhs_is_double ? rhs->floating : (double)rhs->integer;
        vm->registers[0].floating = b + a;
        vm->registers[0].type = V_DOUBLE;
        break;
      }

      if (!(lhs->type == V_INT && rhs->type == V_INT)) {
        Str l = VALUE_TYPE_MAP[lhs->type];
        Str r = VALUE_TYPE_MAP[rhs->type];
        VM_ERR("Can not perform `%.*s` + `%.*s`", (int)l.len, l.p, (int)r.len,
               r.p);
      }

      vm->registers[0].integer = rhs->integer + lhs->integer;
      vm->registers[0].type = V_INT;
      break;
    }

    case OP_SUB: {
      Value *lhs = &vm->registers[0];
      Value *rhs = &vm->registers[arg];

      int lhs_is_double = lhs->type == V_DOUBLE;
      int rhs_is_double = rhs->type == V_DOUBLE;

      if (lhs_is_double | rhs_is_double) {
        double a = lhs_is_double ? lhs->floating : (double)lhs->integer;
        double b = rhs_is_double ? rhs->floating : (double)rhs->integer;
        vm->registers[0].floating = b - a;
        vm->registers[0].type = V_DOUBLE;
        break;
      }

      if (!(lhs->type == V_INT && rhs->type == V_INT)) {
        Str l = VALUE_TYPE_MAP[lhs->type];
        Str r = VALUE_TYPE_MAP[rhs->type];
        VM_ERR("Can not perform `%.*s` - `%.*s`", (int)l.len, l.p, (int)r.len,
               r.p);
      }

      vm->registers[0].integer = rhs->integer - lhs->integer;
      vm->registers[0].type = V_INT;
      break;
    }

    case OP_MUL: {
      Value *lhs = &vm->registers[0];
      Value *rhs = &vm->registers[arg];

      int lhs_is_double = lhs->type == V_DOUBLE;
      int rhs_is_double = rhs->type == V_DOUBLE;

      if (lhs_is_double | rhs_is_double) {
        double a = lhs_is_double ? lhs->floating : (double)lhs->integer;
        double b = rhs_is_double ? rhs->floating : (double)rhs->integer;
        vm->registers[0].floating = b * a;
        vm->registers[0].type = V_DOUBLE;
        break;
      }

      if (!(lhs->type == V_INT && rhs->type == V_INT)) {
        Str l = VALUE_TYPE_MAP[lhs->type];
        Str r = VALUE_TYPE_MAP[rhs->type];
        VM_ERR("Can not perform `%.*s` * `%.*s`", (int)l.len, l.p, (int)r.len,
               r.p);
      }

      vm->registers[0].integer = rhs->integer * lhs->integer;
      vm->registers[0].type = V_INT;
      break;
    }

    case OP_DIV: {
      Value *lhs = &vm->registers[0];
      Value *rhs = &vm->registers[arg];

      int lhs_is_double = lhs->type == V_DOUBLE;
      int rhs_is_double = rhs->type == V_DOUBLE;

      if (lhs_is_double | rhs_is_double) {
        double a = lhs_is_double ? lhs->floating : (double)lhs->integer;
        double b = rhs_is_double ? rhs->floating : (double)rhs->integer;
        vm->registers[0].floating = b / a;
        vm->registers[0].type = V_DOUBLE;
        break;
      }

      if (!(lhs->type == V_INT && rhs->type == V_INT)) {
        Str l = VALUE_TYPE_MAP[lhs->type];
        Str r = VALUE_TYPE_MAP[rhs->type];
        VM_ERR("Can not perform `%.*s` / `%.*s`", (int)l.len, l.p, (int)r.len,
               r.p);
      }

      if (lhs->integer == 0) {
        VM_ERR("Division by zero");
      }

      vm->registers[0].integer = rhs->integer / lhs->integer;
      vm->registers[0].type = V_INT;
      break;
    }
    case OP_EQ: {
      // pointer comparison fast path
      vm->registers[0] = Value_cmp(&vm->registers[0], &vm->registers[arg])
                             ? vm->globals[1]
                             : vm->globals[0];
      break;
    }
    case OP_LT: {
      Value lhs = vm->registers[arg];
      Value rhs = vm->registers[0];
      if (!((1 << lhs.type) & V_NUM_MASK) || !((1 << rhs.type) & V_NUM_MASK)) {
        Str l = VALUE_TYPE_MAP[lhs.type];
        Str r = VALUE_TYPE_MAP[rhs.type];
        VM_ERR("Can not perform `%.*s` < `%.*s`", (int)l.len, l.p, (int)r.len,
               r.p);
      }

      double a = lhs.type == V_INT ? (double)lhs.integer : lhs.floating;
      double b = rhs.type == V_INT ? (double)rhs.integer : rhs.floating;
      vm->registers[0] = (Value){
          .type = (a < b) ? V_TRUE : V_FALSE,
      };
      break;
    }
    case OP_GT: {
      Value lhs = vm->registers[arg];
      Value rhs = vm->registers[0];
      if (!((1 << lhs.type) & V_NUM_MASK) || !((1 << rhs.type) & V_NUM_MASK)) {
        Str l = VALUE_TYPE_MAP[lhs.type];
        Str r = VALUE_TYPE_MAP[rhs.type];
        VM_ERR("Can not perform `%.*s` > `%.*s`", (int)l.len, l.p, (int)r.len,
               r.p);
      }

      double a = lhs.type == V_INT ? (double)lhs.integer : lhs.floating;
      double b = rhs.type == V_INT ? (double)rhs.integer : rhs.floating;
      vm->registers[0] = (Value){
          .type = (a > b) ? V_TRUE : V_FALSE,
      };
      break;
    }
    case OP_IDX: {
      Value target = vm->registers[arg];
      Value idx = vm->registers[0];
      switch (target.type) {
      case V_ARRAY:
        if (idx.type != V_INT || idx.type != V_INT) {
          goto err;
        }
        vm->registers[0] = LIST_get_UNSAFE(target.array, Value_as_int(&idx));
        break;
      case V_OBJ:
        if (idx.type != V_STR) {
          goto err;
        }
        vm->registers[0] = Map_get(target.obj, idx.string);
        break;
      err:
      default:
        Str t = VALUE_TYPE_MAP[target.type];
        Str i = VALUE_TYPE_MAP[idx.type];
        VM_ERR("Cant index into `%.*s` with `%.*s`", (int)t.len, t.p,
               (int)i.len, i.p);
      }
      break;
    }
    case OP_ARGS:
      vm->arg_count = DECODE_ARG_COUNT(arg);
      vm->arg_offset = DECODE_ARG_OFFSET(arg);
      break;
    case OP_SYS: {
      // at this point all builtins are just "syscalls" into an array of
      // function pointers
      ((builtin_function)vm->builtins[arg])(vm);
      vm->arg_count = 1;
      vm->arg_offset = 0;
      break;
    }
    case OP_CALL: {
      Frame *f = freelist_pop(fl);
      f->prev = vm->frame;
      f->return_to_bytecode = vm->pc;
      vm->frame = f;
      vm->pc = arg;
      vm->arg_count = 1;
      vm->arg_offset = 0;
      break;
    }
    case OP_LEAVE: {
      Frame *old = vm->frame;
      if (vm->frame->prev) {
        vm->pc = vm->frame->return_to_bytecode;
        vm->frame = vm->frame->prev;
      }
      freelist_push(fl, old);
      break;
    }
    case OP_JMPF: {
      if (vm->registers[0].type == V_FALSE) {
        vm->pc = arg;
        continue;
      }
      break;
    }
    case OP_JMP: {
      vm->pc = arg;
      continue;
    }
    default:
      VM_ERR("Unimplemented instruction `%.*s`", (int)OP_MAP[op].len,
             OP_MAP[op].p)
    }
    vm->pc += 2;
  }
  return 0;
vm_end:
  return 1;
}

void Vm_destroy(Vm *vm) {
  if (vm->alloc != NULL) {
    CALL(vm->alloc, destroy);
    free(vm->alloc);
  }
}
