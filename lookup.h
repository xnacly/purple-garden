#ifndef LOOKUP_H
#define LOOKUP_H

// TODO: codegen this:

#define BUILTIN_LOOKUP                                                         \
  LOOKUP_ROOT(s, LOOKUP_CASE("len", BUILTIN_LEN)                               \
                     LOOKUP_CASE("print", BUILTIN_PRINT)                       \
                         LOOKUP_CASE("println", BUILTIN_PRINTLN));

#define LOOKUP_ROOT(STR_TO_MATCH, LOOKUP_CASES)                                \
  Builtin b = BUILTIN_UNKOWN;                                                  \
  Str __s = STR_TO_MATCH;                                                      \
  do {                                                                         \
    switch (__s.len + 1) {                                                     \
      LOOKUP_CASES                                                             \
    default: {                                                                 \
      printf("Unknown builtin: `@");                                           \
      Str_debug(&s);                                                           \
      puts("`");                                                               \
      exit(1);                                                                 \
    }                                                                          \
    }                                                                          \
  } while (0)

#define LOOKUP_CASE(STR, BUILTIN_)                                             \
  case sizeof(STR): {                                                          \
    if (memcmp(__s.p, STR, __s.len) == 0) {                                    \
      b = BUILTIN_;                                                            \
      break;                                                                   \
    }                                                                          \
    break;                                                                     \
  }

#endif
