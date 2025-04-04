#include <getopt.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <time.h>

#include "cc.h"
#include "common.h"
#include "io.h"
#include "lexer.h"
#include "parser.h"
#include "vm.h"

#define CTX "pre"
#define VERSION "alpha"
#ifndef COMMIT
#define COMMIT "(no commit)"
#endif

#ifndef BENCH
#define BENCH 0
#endif

typedef struct {
  // options

  // use block allocator before starting a garden, instead of gc; int because
  // getopt has no bool support (not yet supported)
  int _alloc_block;
  // readable bytecode representation with labels, globals and comments
  int disassemble;

  // options in which we exit after toggle
  int version;
  int help;

  // entry point - last argument thats not an option
  char *filename;
} Args;

typedef struct {
  char *name_long;
  char name_short;
  char *description;
} cli_option;

// WARN: DO NOT REORDER THIS - will result in option handling issues
static const cli_option options[] = {
    {"disassemble", 'd',
     "readable bytecode representation with labels, globals and comments"},
    {"version", 'v', "display version information"},
    {"help", 'h', "extended usage information"},
};

void usage() {
  fprintf(stderr, "usage: purple_garden ");
  size_t len = sizeof(options) / sizeof(cli_option);
  for (size_t i = 0; i < len; i++) {
    fprintf(stderr, "[-%c | --%s] ", options[i].name_short,
            options[i].name_long);
  }
  fprintf(stderr, "<file.garden>\n");
}

Args Args_parse(int argc, char **argv) {
  Args a = (Args){0};
  // MUST be in sync with options, otherwise this will not work as intended
  struct option long_options[] = {
      {options[0].name_long, no_argument, &a.disassemble, 1},
      {options[1].name_long, no_argument, &a.version, 1},
      {options[2].name_long, no_argument, &a.help, 1},
      {0, 0, 0, 0},
  };

  int opt;
  while ((opt = getopt_long(argc, argv, "dvh", long_options, NULL)) != -1) {
    switch (opt) {
    case 'd':
      a.disassemble = 1;
      break;
    case 'h':
      a.help = 1;
      break;
    case 'v':
      a.version = 1;
      break;
    case 0:
      break;
    default:
      usage();
      exit(EXIT_FAILURE);
    }
  }

  if (optind < argc) {
    a.filename = argv[optind];
  }

  // command handling
  if (a.version) {
    fprintf(stderr, "purple_garden: %s-%s-%s\n", CTX, VERSION, COMMIT);
#ifdef COMMIT_MSG
    fprintf(stderr, "with commit=`" COMMIT_MSG "`\n");
#endif
    exit(EXIT_SUCCESS);
  } else if (a.help) {
    fprintf(stderr, "purple_garden: %s-%s-%s\n", CTX, VERSION, COMMIT);
    usage();
    size_t len = sizeof(options) / sizeof(cli_option);
    fprintf(stderr, "\nOptions:\n");
    for (size_t i = 0; i < len; i++) {
      fprintf(stderr, "\t-%c,--%-15s %s\n", options[i].name_short,
              options[i].name_long, options[i].description);
    }
    exit(EXIT_SUCCESS);
  }

  if (a.filename == NULL) {
    usage();
    fprintf(stderr, "Wanted a filename as an argument, not enough arguments\n");
    exit(EXIT_FAILURE);
  };

  return a;
}

#if BENCH
#define BENCH_PUTS(msg)                                                        \
  {                                                                            \
    size_t cur = clock();                                                      \
    printf("[BENCH] (T-%.4fms): " msg "\n",                                    \
           ((clock() - start) / (double)CLOCKS_PER_SEC) * 1000);               \
    start = cur;                                                               \
  }
#else
#define BENCH_PUTS(msg)
#endif

int main(int argc, char **argv) {
  // TODO("ADD SUPPORT FOR T_AT");
#if BENCH
  size_t start = clock();
#endif
  Args a = Args_parse(argc, argv);

  BENCH_PUTS("parsed arguments");

  Str input = IO_read_file_to_string(a.filename);
#if DEBUG
  puts("================== IN ==================");
  Str_debug(&input);
#endif
  BENCH_PUTS("read file into memory");

  Lexer l = Lexer_new(input);

#if DEBUG
  puts("================= TOKS =================");
#endif
  Parser p = Parser_new(&l);
  Node ast = Parser_run(&p);
  BENCH_PUTS("parsed input");

#if DEBUG
  puts("================= TREE =================");
  Node_debug(&ast, 0);
  puts("");
#endif

  Vm vm = cc(&ast);
#if DEBUG
  puts("================= DASM =================");
  a.disassemble = 1;
#endif
  if (a.disassemble) {
    disassemble(&vm);
    puts("");
  }
  Node_destroy(&ast);
  BENCH_PUTS("compiled input");
#if BENCH
  printf("[BENCH] (bc=%zu|globals=%zu)\n", vm.bytecode_len, vm.global_len);
#endif

  int runtime_code = Vm_run(&vm);
  BENCH_PUTS("ran vm");

  Vm_destroy(vm);
  munmap(input.p, input.len);
  BENCH_PUTS("destroyed Nodes, vm and input");

  return runtime_code == 0 ? EXIT_SUCCESS : EXIT_FAILURE;
}
