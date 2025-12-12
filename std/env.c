#include "std.h"

static Map *env = NULL;
extern char **environ;

static void setup_env(Vm_Config conf, Allocator *a) {
  env = CALL(a, request, sizeof(Map));
  Map env_map = Map_new(128, a);
  if (!conf.no_env) {
    for (char **e = environ; *e; e++) {
      const char *s = *e;

      // separator =
      const char *eq = strchr(s, '=');
      if (!eq) {
        continue;
      }

      Str key = {.p = (const uint8_t *)s, .len = eq - s};
      key.hash = Str_hash(&key);
      Str *val = CALL(a, request, sizeof(Str));
      *val = (Str){.p = (const uint8_t *)eq + 1, .len = strlen(eq + 1)};

      Map_insert(&env_map, &key, (Value){.type = V_STR, .string = val}, a);
    }
  }
  *env = env_map;
}

static void builtin_env_get(Vm *vm) {
  Value key = ARG(0);
  ASSERT(key.type == V_STR, "Env idx must be string");
  RETURN(Map_get(env, key.string));
}

static void builtin_env_set(Vm *vm) {
  Value key = ARG(0);
  Value value = ARG(1);
  ASSERT(key.type == V_STR, "Env idx must be string");
  ASSERT(value.type == V_STR, "Env val must be string");
  Map_insert(env, key.string, value, vm->staticalloc);
}
