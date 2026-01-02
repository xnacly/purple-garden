#include "../vm.h"

static void pg_builtin_arr_range(Vm *vm) {
  int64_t start = Value_as_int(&ARG(0));
  int64_t end = Value_as_int(&ARG(1));

  if (end < start) {
    List *l = gc_request(vm->gc, sizeof(List), GC_OBJ_LIST);
    *l = List_new(0, vm->gc);
    RETURN((Value){.type = V_ARRAY, .array = l});
  }

  size_t cap = end - start;
  List *l = gc_request(vm->gc, sizeof(List), GC_OBJ_LIST);
  *l = List_new(0, vm->gc);

  for (size_t i = 0; i < cap; i++) {
    l->arr[i] = (Value){.type = V_INT, .integer = start + i};
  }

  l->len = cap;

  RETURN((Value){.type = V_ARRAY, .array = l});
}

static void pg_builtin_arr_new(Vm *vm) {
  Value size = ARG(0);
  List *l = gc_request(vm->gc, sizeof(List), GC_OBJ_LIST);
  *l = List_new((size_t)Value_as_int(&size), vm->gc);
  RETURN((Value){.type = V_ARRAY, .array = l});
}
