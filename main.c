#include <getopt.h>
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
  // use block allocator before starting a garden, instead of gc; int because
  // getopt has no bool support
  int alloc_block;
  // enable debug logs
  int debug;
  int version;
  // entry garden
  char *filename;
} Args;

static const char *options[] = {
    "alloc-block",
    "debug",
    "version",
};

void usage() {
  fprintf(stderr, "usage: purple_garden ");
  size_t len = sizeof(options) / sizeof(char *);
  for (size_t i = 0; i < len; i++) {
    fprintf(stderr, "[--%s] ", options[i]);
  }
  fprintf(stderr, "<file.garden>\n");
}

Args Args_parse(int argc, char **argv) {
  Args a = (Args){0};
  struct option long_options[] = {{options[0], no_argument, &a.alloc_block, 1},
                                  {options[1], no_argument, &a.debug, 1},
                                  {options[2], no_argument, &a.version, 1},
                                  {0, 0, 0, 0}};

  int opt;
  while ((opt = getopt_long(argc, argv, "", long_options, NULL)) != -1) {
    switch (opt) {
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
#if BENCH
  size_t start = clock();
#endif
  Args a = Args_parse(argc, argv);
  if (a.version) {
    fprintf(stderr, "purple_garden: %s-%s-%s\n", CTX, VERSION, COMMIT);
    return EXIT_SUCCESS;
  }
  if (a.filename == NULL) {
    usage();
    ASSERT(a.filename != NULL,
           "Wanted a filename as an argument, not enough arguments")
  };
  BENCH_PUTS("parsed arguments");

  String input = IO_read_file_to_string(a.filename);
#if DEBUG
  puts("================== IN ==================");
  printf(input.p);
#endif
  BENCH_PUTS("read file to String");

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
  BENCH_PUTS("compiled input");
#if BENCH
  printf("[BENCH] (bc=%zu|globals=%zu)\n", vm.bytecode_len, vm.global_len);
#endif

  int runtime_code = Vm_run(&vm);
  BENCH_PUTS("ran vm");

  Node_destroy(&ast);
  Vm_destroy(vm);
  munmap(input.p, input.len);
  BENCH_PUTS("destroyed Nodes, vm and input");

  return runtime_code == 0 ? EXIT_SUCCESS : EXIT_FAILURE;
}
