#include "6cl.h"
#include <float.h>
#include <limits.h>
#include <stdlib.h>
#include <string.h>

#define __HASH_TABLE_SIZE 512
#define __HASH_TABLE_MASK (__HASH_TABLE_SIZE - 1)

// SixStr is a 0 copy window into a string
typedef struct {
  const char *p;
  size_t len;
} SixStr;

#define SIX_STR_NEW(CSTR) (

#include <stdio.h>

static char *SIX_FLAG_TYPE_TO_MAP[] = {
    [SIX_STR] = "string",    [SIX_BOOL] = "bool", [SIX_CHAR] = "char",
    [SIX_INT] = "int",       [SIX_LONG] = "long", [SIX_FLOAT] = "float",
    [SIX_DOUBLE] = "double",
};

void print_flag(SixFlag *f, bool long_option) {
  char *pre_and_postfix = "[]";
  if (long_option) {
    putc('\t', stdout);
    pre_and_postfix = "  ";
  }

  printf("%c %c%c / %c%s", pre_and_postfix[0], SIX_OPTION_PREFIX, f->short_name,
         SIX_OPTION_PREFIX, f->name);
  if (f->type != SIX_BOOL) {
    printf(" <%s=", SIX_FLAG_TYPE_TO_MAP[f->type]);
    switch (f->type) {
    case SIX_STR:
      printf("`%s`", f->s);
      break;
    case SIX_CHAR:
      putc(f->c, stdout);
      break;
    case SIX_INT:
      printf("%d", f->i);
      break;
    case SIX_LONG:
      printf("%ld", f->l);
      break;
    case SIX_FLOAT:
      printf("%g", f->f);
      break;
    case SIX_DOUBLE:
      printf("%g", f->d);
      break;
    default:
    }
    putc('>', stdout);
  }
  putc(pre_and_postfix[1], stdout);
  putc(' ', stdout);

  if (long_option) {
    if (f->description) {
      printf("\n\t\t%s\n", f->description);
    }
    putc('\n', stdout);
  }
}

static SixFlag HELP_FLAG = {
    .name = "help",
    .short_name = 'h',
    .description = "help page and usage",
    .type = SIX_BOOL,
};

// part of -h, --help, +h, +help and any unknown option
static void usage(const char *pname, const Six *h) {
  // should i put this to stdout or stderr
  printf("usage %s: ", pname);
  size_t len = strlen(pname) + 7;
  for (size_t i = 0; i < h->flag_count; i++) {
    print_flag(&h->flags[i], false);
    if ((i + 1) % 2 == 0 && i + 1 < h->flag_count) {
      printf("\n%*.s ", (int)len, "");
    }
  }

  printf("\n%*.s ", (int)len, "");
  print_flag(&HELP_FLAG, false);

  if (h->name_for_rest_arguments) {
    puts(h->name_for_rest_arguments);
  } else {
    puts("");
  }
}

static void help(const char *pname, const Six *h) {
  usage(pname, h);
  size_t len = strlen(pname);
  printf("\nOption:\n");
  for (size_t j = 0; j < h->flag_count; j++) {
    print_flag(&h->flags[j], true);
  }
  print_flag(&HELP_FLAG, true);

  printf("Examples: ");
  for (size_t i = 0; i < 2; i++) {
    printf("\n\t%s ", pname);
    for (size_t j = 0; j < h->flag_count; j++) {
      SixFlag *s = &h->flags[j];
      if (i) {
        printf("%c%s", SIX_OPTION_PREFIX, s->name);
      } else {
        printf("%c%c", SIX_OPTION_PREFIX, s->short_name);
      }
      switch (s->type) {
      case SIX_STR:
        printf(" \"%s\"", s->s);
        break;
      case SIX_CHAR:
        printf(" %c", s->c);
        break;
      case SIX_INT:
        printf(" %d", s->i);
        break;
      case SIX_LONG:
        printf(" %zu", s->l);
        break;
      case SIX_FLOAT:
      case SIX_DOUBLE:
        printf(" %g", s->f);
        break;
      case SIX_BOOL:
      default:
        break;
      }
      putc(' ', stdout);
      if ((j + 1) % 2 == 0 && j + 1 < h->flag_count) {
        printf("\\\n\t %*.s", (int)len, "");
      }
    }
    puts("");
  }
}

static size_t fnv1a(const char *str, size_t len) {
#define FNV_OFFSET_BASIS 0x811c9dc5
#define FNV_PRIME 0x01000193

  size_t hash = FNV_OFFSET_BASIS;
  for (size_t i = 0; i < len; i++) {
    hash ^= str[i];
    hash *= FNV_PRIME;
  }

  return hash;
}

static int process_argument(SixFlag *f, size_t cur, size_t argc, char **argv) {
  size_t offset = 1;
  switch (f->type) {
  case SIX_STR: {
    if (cur + 1 >= argc) {
      fprintf(stderr, "No STRING value for option '%s'\n", f->name);
      return -1;
    }
    f->s = argv[cur + 1];
    break;
  }
  case SIX_BOOL:
    f->b = true;
    offset = 0;
    break;
  case SIX_CHAR:
    if (cur + 1 >= argc) {
      fprintf(stderr, "No char value found for option '%s/%c'\n", f->name,
              f->short_name);
      return -1;
    } else if (argv[cur + 1][0] == '\0') {
      fprintf(stderr, "No char found for option '%s/%c', empty argument\n",
              f->name, f->short_name);
      return -1;
    } else if (argv[cur + 1][1] != '\0') {
      fprintf(stderr,
              "'%s/%c' value has too many characters, want one for type CHAR\n",
              f->name, f->short_name);
      return -1;
    }
    f->c = argv[cur + 1][0];
    break;
  case SIX_INT: {
    if (cur + 1 >= argc) {
      fprintf(stderr, "No INT value for option '%s/%c'\n", f->name,
              f->short_name);
      return -1;
    }
    char *tmp = argv[cur + 1];
    char *endptr = NULL;
    int errno = 0;
    long val = strtol(tmp, &endptr, 10);

    if (endptr == tmp || *endptr != '\0') {
      fprintf(stderr, "Invalid integer for option '%s/%c': '%s'\n", f->name,
              f->short_name, tmp);
      return -1;
    }

    if (val < INT_MIN || val > INT_MAX) {
      fprintf(stderr, "Integer out of range for option '%s/%c': %ld\n", f->name,
              f->short_name, val);
      return -1;
    }

    f->i = (int)val;
    break;
  }
  case SIX_LONG: {
    if (cur + 1 >= argc) {
      fprintf(stderr, "No LONG value for option '%s/%c'\n", f->name,
              f->short_name);
      return -1;
    }
    char *tmp = argv[cur + 1];
    char *endptr = NULL;
    int errno = 0;
    long val = strtol(tmp, &endptr, 10);

    if (endptr == tmp || *endptr != '\0') {
      fprintf(stderr, "Invalid LONG integer for option '%s/%c': '%s'\n",
              f->name, f->short_name, tmp);
      return -1;
    }

    if (val < LONG_MIN || val > LONG_MAX) {
      fprintf(stderr, "LONG integer out of range for option '%s/%c': %ld\n",
              f->name, f->short_name, val);
      return -1;
    }

    f->l = val;
    break;
  }
  case SIX_FLOAT: {
    if (cur + 1 >= argc) {
      fprintf(stderr, "No FLOAT value for option '%s/%c'\n", f->name,
              f->short_name);
      return -1;
    }
    char *tmp = argv[cur + 1];
    char *endptr = NULL;
    int errno = 0;
    long val = strtof(tmp, &endptr);

    if (endptr == tmp || *endptr != '\0') {
      fprintf(stderr, "Invalid FLOAT for option '%s/%c': '%s'\n", f->name,
              f->short_name, tmp);
      return -1;
    }

    if (val < FLT_MIN || val > FLT_MAX) {
      fprintf(stderr, "FLOAT out of range for option '%s/%c': %ld\n", f->name,
              f->short_name, val);
      return -1;
    }

    f->f = val;
    break;
  }
  case SIX_DOUBLE: {
    if (cur + 1 >= argc) {
      fprintf(stderr, "No DOUBLE value for option '%s/%c'\n", f->name,
              f->short_name);
      return -1;
    }
    char *tmp = argv[cur + 1];
    char *endptr = NULL;
    int errno = 0;
    long val = strtod(tmp, &endptr);

    if (endptr == tmp || *endptr != '\0') {
      fprintf(stderr, "Invalid DOUBLE for option '%s/%c': '%s'\n", f->name,
              f->short_name, tmp);
      return -1;
    }

    if (val < FLT_MIN || val > FLT_MAX) {
      fprintf(stderr, "DOUBLE out of range for option '%s/%c': %ld\n", f->name,
              f->short_name, val);
      return -1;
    }

    f->d = val;
    break;
  }
  default:
    fprintf(stderr, "Unknown type for option '%s/%c'\n", f->name,
            f->short_name);
    return -1;
  }

  return offset;
}

void SixParse(Six *six, size_t argc, char **argv) {
  SixStr help_str = {.p = "help", .len = sizeof("help") - 1};

  // maps a strings hash to its index into the option array
  short hash_table_long[__HASH_TABLE_SIZE] = {0};

  // ASCII, since there is just these for short options, there should be even
  // less, since we dont really support nonprintable chars, but yeah, also we
  // zero this
  short table_short[256] = {0};

  // registering all options
  for (size_t i = 0; i < six->flag_count; i++) {
    SixFlag *f = &six->flags[i];

    // we increment the index by one here, since we use all tables and arrays
    // zero indexed, distinguishing between a not found and the option at index
    // 0 is therefore clear
    hash_table_long[fnv1a(f->name, strnlen(f->name, 256)) & __HASH_TABLE_MASK] =
        i + 1;
    if (f->short_name) {
      table_short[(int)f->short_name] = i + 1;
    }
  }

  // i = 1 since we are skipping the process name (pname)
  for (size_t i = 1; i < argc; i++) {
    SixStr arg_cur = (SixStr){.p = (argv[i]), .len = strnlen(argv[i], 256)};

    // not starting with PREFIX means: no option, thus rest
    if (arg_cur.p[0] != SIX_OPTION_PREFIX) {
      if (six->rest_count + 1 >= SIX_MAX_REST) {
        fprintf(stderr, "Not enough space left for more rest arguments\n");
        goto err;
      }
      six->rest[six->rest_count++] = argv[i];
      continue;
    }

    // check if short option
    if (arg_cur.len == 2) {
      int cc = arg_cur.p[1];
      if (cc > 256 || cc < 0) {
        fprintf(stderr, "Unkown short option '%c'\n", arg_cur.p[1]);
        goto err;
      }

      // single char option usage/help page
      if (cc == 'h') {
        help(argv[0], six);
        exit(EXIT_SUCCESS);
      }

      // check if short option is a registered one
      short option_idx = table_short[(int)cc];
      if (!option_idx) {
        fprintf(stderr, "Unkown short option '%c'\n", arg_cur.p[1]);
        goto err;
      }

      // we decrement option_idx, since we zero the lookup table, thus an
      // empty value is 0 and the index of the first option is 1, we correct
      // this here
      option_idx--;

      int offset = process_argument(&six->flags[option_idx], i, argc, argv);
      if (offset == -1) {
        goto err;
      }
      i += offset;
    } else {
      // strip first char by moving the start of the window one to the right
      arg_cur.p++;
      arg_cur.len--;

      // long help page with option description and stuff
      if (strncmp(arg_cur.p, help_str.p, help_str.len) == 0) {
        help(argv[0], six);
        exit(EXIT_SUCCESS);
      }

      size_t idx =
          hash_table_long[fnv1a(arg_cur.p, arg_cur.len) & __HASH_TABLE_MASK];
      if (!idx) {
        fprintf(stderr, "Unkown option '%*s'\n", (int)arg_cur.len, arg_cur.p);
        goto err;
      }

      // decrement idx since we use 0 as the no option value
      idx--;

      SixFlag *f = &six->flags[idx];
      int offset = process_argument(f, i, argc, argv);
      if (offset == -1) {
        goto err;
      }
      i += offset;
    }
  }

  return;

err:
  usage(argv[0], six);
  exit(EXIT_FAILURE);
  return;
}
