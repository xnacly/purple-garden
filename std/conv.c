#include "../vm.h"

// reimplementation of Str_to_int64_t but with input verification, since
// Str_to_int64_t expects the lexer to verify all inputs. Also supports prefix
// via +-.
static void pg_builtin_conv_int(Vm *vm) {
  Value in = ARG(0);
  const Str *s = in.string;
  if (!s->len)
    goto err;

  size_t i = 0;
  bool neg = false;

  if (s->p[0] == '-') {
    neg = true;
    i = 1;
    if (s->len == 1)
      goto err;
  } else if (s->p[0] == '+') {
    i = 1;
    if (s->len == 1)
      goto err;
  }

  int64_t result = 0;

  for (; i < s->len; i++) {
    unsigned char c = s->p[i];
    if (c < '0' || c > '9')
      goto err;

    int digit = c - '0';

    if (!neg) {
      if (result > (INT64_MAX - digit) / 10)
        goto err;
      result = result * 10 + digit;
    } else {
      if (result < (INT64_MIN + digit) / 10)
        goto err;
      result = result * 10 - digit;
    }
  }

  RETURN((Value){.type = V_INT, .integer = result, .is_some = true});
  return;

err:
  RETURN((Value){.type = V_NONE});
}

static void pg_builtin_conv_num(Vm *vm) { TODO("builtin_conv_num") }

static void pg_builtin_conv_str(Vm *vm) { TODO("builtin_conv_str") }
