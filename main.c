#include <getopt.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <sys/time.h>

#include "cc.h"
#include "common.h"
#include "io.h"
#include "lexer.h"
#include "mem.h"
#include "parser.h"
#include "vm.h"

#define CTX "pre"
#define VERSION "alpha"

#ifndef COMMIT
#define COMMIT "(no commit associated)"
#endif

#ifndef COMMIT_MSG
#define COMMIT_MSG "(no commit message associated)"
#endif

#define VERBOSE_PUTS(fmt, ...)                                                 \
  do {                                                                         \
    if (UNLIKELY(a.verbose)) {                                                 \
      gettimeofday(&end_time, NULL);                                           \
      double elapsed_time =                                                    \
          (end_time.tv_sec - start_time.tv_sec) +                              \
          (end_time.tv_usec - start_time.tv_usec) / 1000000.0;                 \
      printf("[%10.4fms] " fmt "\n", elapsed_time * 1000, ##__VA_ARGS__);      \
      gettimeofday(&start_time, NULL);                                         \
    }                                                                          \
  } while (0)

typedef struct {
  // options - int because getopt has no bool support

  // use block allocator instead of garbage collection
  int block_allocator;
  // compile all functions to machine code
  int aot_functions;
  // readable bytecode representation with labels, globals and comments
  int disassemble;
  // display the memory usage of parsing, compilation and the virtual machine
  int memory_usage;

  // verbose logging
  int verbose;

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
    {"version", 'v', "display version information"},
    {"help", 'h', "extended usage information"},
    {"disassemble", 'd',
     "readable bytecode representation with labels, globals and comments"},
    {"block-allocator", 'b',
     "use block allocator instead of garbage collection"},
    {"aot-functions", 'a', "compile all functions to machine code"},
    {"memory-usage", 'm',
     "display the memory usage of parsing, compilation and the virtual "
     "machine"},
    {"verbose", 'V', "verbose logging"},
};

void usage() {
  Str prefix = STRING("usage: purple_garden");
  printf("%.*s ", (int)prefix.len, prefix.p);
  size_t len = sizeof(options) / sizeof(cli_option);
  for (size_t i = 0; i < len; i++) {
    printf("[-%c | --%s] ", options[i].name_short, options[i].name_long);
    if ((i + 1) % 3 == 0 && i + 1 < len) {
      printf("\n%*.s ", (int)prefix.len, "");
    }
  }
  printf("<file.garden>\n");
}

Args Args_parse(int argc, char **argv) {
  Args a = (Args){0};
  // MUST be in sync with options, otherwise this will not work as intended
  struct option long_options[] = {
      {options[0].name_long, no_argument, &a.version, 1},
      {options[1].name_long, no_argument, &a.help, 1},
      {options[2].name_long, no_argument, &a.disassemble, 1},
      {options[3].name_long, no_argument, &a.block_allocator, 1},
      {options[4].name_long, no_argument, &a.aot_functions, 1},
      {options[5].name_long, no_argument, &a.memory_usage, 1},
      {options[6].name_long, no_argument, &a.verbose, 1},
      {0, 0, 0, 0},
  };

  int opt;
  while ((opt = getopt_long(argc, argv, "vhdbamV", long_options, NULL)) != -1) {
    switch (opt) {
    case 'v':
      a.version = 1;
      break;
    case 'V':
      a.verbose = 1;
      break;
    case 'h':
      a.help = 1;
      break;
    case 'd':
      a.disassemble = 1;
      break;
    case 'b':
      a.block_allocator = 1;
      break;
    case 'a':
      a.aot_functions = 1;
      break;
    case 'm':
      a.memory_usage = 1;
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
  if (UNLIKELY(a.version)) {
    printf("purple_garden: %s-%s-%s\n", CTX, VERSION, COMMIT);
    if (UNLIKELY(a.verbose)) {
      puts(COMMIT_MSG);
    }
    exit(EXIT_SUCCESS);
  } else if (UNLIKELY(a.help)) {
    usage();
    size_t len = sizeof(options) / sizeof(cli_option);
    printf("\noptions:\n");
    for (size_t i = 0; i < len; i++) {
      printf("\t-%c, --%-15s %s\n", options[i].name_short, options[i].name_long,
             options[i].description);
    }
    exit(EXIT_SUCCESS);
  }

  if (UNLIKELY(a.filename == NULL)) {
    usage();
    fprintf(stderr, "error: Missing a file? try `-h/--help`\n");
    exit(EXIT_FAILURE);
  };

  return a;
}

int main(int argc, char **argv) {
  struct timeval start_time, end_time;
  Args a = Args_parse(argc, argv);
  if (UNLIKELY(a.verbose)) {
    gettimeofday(&start_time, NULL);
  }
  VERBOSE_PUTS("main::Args_parse: Parsed arguments");

  Str input = IO_read_file_to_string(a.filename);
  VERBOSE_PUTS("io::IO_read_file_to_string: mmaped input of size=%zuB",
               input.len);
#if DEBUG
  puts("================== INPUTS ==================");
  Str_debug(&input);
#endif

  // this allocator stores both nodes, bytecode and the global pool of the vm,
  // thus it has to life exactly as long as the vm does.
  Allocator pipeline_allocator = {
      .init = bump_init,
      .request = bump_request,
      .destroy = bump_destroy,
      .reset = bump_reset,
      .stats = bump_stats,
  };
  size_t file_size_or_min = (input.len < MIN_MEM ? MIN_MEM : input.len);
  size_t min_size = (
      // size for globals
      (file_size_or_min * sizeof(Value))
      // size for bytecode
      + file_size_or_min
      // size for nodes
      + (file_size_or_min * sizeof(Node)));
  pipeline_allocator.ctx = pipeline_allocator.init(min_size);
  VERBOSE_PUTS("mem::init: Allocated memory block of size=%zuB", min_size);
  Lexer l = Lexer_new(input);
  Parser p = Parser_new(&l, &pipeline_allocator);

  Vm vm = cc(&p);
  VERBOSE_PUTS("cc::cc: Flattened AST to byte code/global pool length=%zu/%zu",
               vm.bytecode_len, vm.global_len);
#if DEBUG
  puts("================= DISASM =================");
  a.disassemble = 1;
#endif

  if (UNLIKELY(a.disassemble)) {
    disassemble(&vm);
    puts("");
  }

  if (UNLIKELY(a.memory_usage)) {
    Stats s = pipeline_allocator.stats(pipeline_allocator.ctx);
    double percent = (s.current * 100) / (double)s.allocated;
    printf("%.2fKB of %.2fKB used (%f%%)\n", s.current / 1024.0,
           s.allocated / 1024.0, percent);
  }

#if DEBUG
  puts("================= MEMORY =================");
  Stats s = pipeline_allocator.stats(pipeline_allocator.ctx);
  double percent = (s.current * 100) / (double)s.allocated;
  printf("%.2fKB of %.2fKB used (%.2f%%)\n", s.current / 1024.0,
         s.allocated / 1024.0, percent);
#endif

  int runtime_code = Vm_run(&vm);
  VERBOSE_PUTS("vm::Vm_run: executed byte code");

  pipeline_allocator.destroy(pipeline_allocator.ctx);
  VERBOSE_PUTS("mem::Allocator::destroy: Deallocated memory space");

  Vm_destroy(vm);
  VERBOSE_PUTS("vm::Vm_destroy: teared vm down");

  munmap(input.p, input.len);
  VERBOSE_PUTS("munmap: unmapped input");

  return runtime_code == 0 ? EXIT_SUCCESS : EXIT_FAILURE;
}
