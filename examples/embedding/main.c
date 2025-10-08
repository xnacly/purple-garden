// Obviously we need to access types from the pg runtime
#include "../../pg.h"

static const Str *HELLO_STR = &STRING("hello from the embedding context");

// Each builtin gets a mutable reference to the the vm state, from this the
// builtin can access data like the current registers, argument count, other
// builtins, the bytecode, the callframe, the allocator, etc. This is defined in
// the following type each builtin has to adhere to: typedef void
// (*builtin_function)(Vm *vm);
void builtin_dbg(Vm *vm) {
  // For this simple example we iterate the arguments and print them to stdout
  // via Value_debug
  for (size_t i = 0; i < vm->arg_count; i++) {
    // ARG is used to get the arguments to the builtin, always make sure i <
    // vm->arg_count
    Value v = ARG(i);
    Value_debug(&v);
    puts("");
  }

  // RETURN puts its inner value into the return register of the virtual machine
  RETURN((Value){.type = V_STR, .string = HELLO_STR});
}

int main(void) {
  // pg can be configured by Vm_Config, there are options available like
  // disabling the garbage collector, limiting the memory usage, etc.
  Vm_Config vm_config = {
      .max_memory = MIN_MEM * 2,
      .disable_std_namespace = true,
      .disable_gc = true,
      .remove_default_builtins = false,
  };

  // creates the purple garden context and the virtual machine
  Pg pg = pg_init(&vm_config);

  // register a builtin function in the pg context with the name 'dbg' and
  // taking the pointer to 'builtin_dbg'; The vm will invoke this pointer upon
  // encountering a builtin called 'dbg' in the source code
  PG_REGISTER_BUILTIN(&pg, "dbg", builtin_dbg);

  // pg will map 'hello.garden' into memory and begin the execution pipeline,
  // the return status is 0 on success.
  int status = pg_exec_file(&pg, "hello.garden");

  // this frees all memory used and destroys the context
  pg_destroy(&pg);
  return status;
}
