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
  size_t block_allocator;
  // compile all functions to machine code
  int aot_functions;
  // readable bytecode representation with labels, globals and comments
  int disassemble;
  // display the memory usage of parsing, compilation and the virtual machine
  int memory_usage;

  // executes the argument as if an input file was given
  char *run;

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
  char *arg_name;
} cli_option;

// WARN: DO NOT REORDER THIS - will result in option handling issues
static const cli_option options[] = {
    {"version", 'v', "display version information", ""},
    {"help", 'h', "extended usage information", ""},
    {"disassemble", 'd',
     "readable bytecode representation with labels, globals and comments", ""},
    {"block-allocator", 'b',
     "use block allocator instead of garbage collection", "<size>"},
    {"aot-functions", 'a', "compile all functions to machine code", ""},
    {"memory-usage", 'm',
     "display the memory usage of parsing, compilation and the virtual "
     "machine",
     ""},
    {"verbose", 'V', "verbose logging", ""},
    {"run", 'r', "executes the argument as if an input file was given",
     "<input>"},
};

void usage() {
  Str prefix = STRING("usage: purple_garden");
  printf("%.*s ", (int)prefix.len, prefix.p);
  size_t len = sizeof(options) / sizeof(cli_option);
  for (size_t i = 0; i < len; i++) {
    char *equal_or_not = options[i].arg_name[0] == 0 ? "" : "=";
    char *name_or_not = options[i].arg_name[0] == 0 ? "" : options[i].arg_name;
    printf("[-%c%s | --%s%s%s] ", options[i].name_short, name_or_not,
           options[i].name_long, equal_or_not, name_or_not);
    if ((i + 1) % 2 == 0 && i + 1 < len) {
      printf("\n%*.s ", (int)prefix.len, "");
    }
  }
  printf("<file.garden>\n");
}

// TODO: replace this shit with `6cl` - the purple garden and 6wm arguments
// parser
Args Args_parse(int argc, char **argv) {
  Args a = (Args){0};
  // MUST be in sync with options, otherwise this will not work as intended
  struct option long_options[] = {
      {options[0].name_long, no_argument, &a.version, 1},
      {options[1].name_long, no_argument, &a.help, 1},
      {options[2].name_long, no_argument, &a.disassemble, 1},
      {options[3].name_long, required_argument, 0, 'b'},
      {options[4].name_long, no_argument, &a.aot_functions, 1},
      {options[5].name_long, no_argument, &a.memory_usage, 1},
      {options[6].name_long, no_argument, &a.verbose, 1},
      {options[7].name_long, required_argument, 0, 'r'},
      {0, 0, 0, 0},
  };

  int opt;
  while ((opt = getopt_long(argc, argv, "vhdb:amVr:", long_options, NULL)) !=
         -1) {
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
    case 'r':
      a.run = optarg;
      break;
    case 'b':
      char *endptr;
      size_t block_size = strtol(optarg, &endptr, 10);
      ASSERT(endptr != optarg, "args: Failed to parse number from: %s", optarg);
      a.block_allocator = block_size;
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
    printf("\nOptions:\n");
    for (size_t i = 0; i < len; i++) {
      char *equal_or_not = options[i].arg_name[0] == 0 ? "" : "=";
      char *name_or_not =
          options[i].arg_name[0] == 0 ? "" : options[i].arg_name;
      printf("\t-%c%s%s, --%s%s%s\n\t\t%s\n\n", options[i].name_short,
             equal_or_not, name_or_not, options[i].name_long, equal_or_not,
             name_or_not, options[i].description);
    }
    exit(EXIT_SUCCESS);
  }

  if (UNLIKELY(a.filename == NULL && a.run == NULL)) {
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

  Str input;
  if (a.run != NULL) {
    input = (Str){.p = a.run, .len = strlen(a.run)};
  } else {
    input = IO_read_file_to_string(a.filename);
    VERBOSE_PUTS("io::IO_read_file_to_string: mmaped input of size=%zuB",
                 input.len);
  }
#if DEBUG
  puts("================== INPUTS ==================");
  Str_debug(&input);
  a.disassemble = 1;
  a.memory_usage = 1;
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
                        + (file_size_or_min * sizeof(Node))) *
                    2;

  pipeline_allocator.ctx = pipeline_allocator.init(min_size);
  VERBOSE_PUTS("mem::init: Allocated memory block of size=%zuB", min_size);
  Lexer l = Lexer_new(input);
  Token *tokens = pipeline_allocator.request(pipeline_allocator.ctx,
                                             file_size_or_min * sizeof(Token));
#if DEBUG
  puts("================== TOKENS ==================");
#endif
  size_t count = Lexer_all(&l, tokens);
#if DEBUG
  for (size_t i = 0; i < count; i++) {
    Token_debug(&tokens[i]);
    puts("");
  }
#endif
  VERBOSE_PUTS("lexer::Lexer_all: lexed tokens count=%zu", count);
  if (UNLIKELY(a.memory_usage)) {
    Stats s = pipeline_allocator.stats(pipeline_allocator.ctx);
    double percent = (s.current * 100) / (double)s.allocated;
    printf("lex: %.2fKB of %.2fKB used (%.2f%%)\n", s.current / 1024.0,
           s.allocated / 1024.0, percent);
  }
  Parser p = Parser_new(&pipeline_allocator, tokens);
  size_t node_count = file_size_or_min * sizeof(Node) / 4;
  Node *nodes = pipeline_allocator.request(pipeline_allocator.ctx, node_count);
#if DEBUG
  puts("================== ASTREE ==================");
#endif
  node_count = Parser_all(nodes, &p, node_count);
#if DEBUG
  for (size_t i = 0; i < node_count; i++) {
    Node_debug(&nodes[i], 0);
    puts("");
  }
#endif
  VERBOSE_PUTS("parser::Parser_next created AST with node_count=%zu",
               node_count);
  if (UNLIKELY(a.memory_usage)) {
    Stats s = pipeline_allocator.stats(pipeline_allocator.ctx);
    double percent = (s.current * 100) / (double)s.allocated;
    printf("parse: %.2fKB of %.2fKB used (%.2f%%)\n", s.current / 1024.0,
           s.allocated / 1024.0, percent);
  }
  Vm vm = cc(&pipeline_allocator, nodes, node_count);
  VERBOSE_PUTS("cc::cc: Flattened AST to byte code/global pool length=%zu/%zu",
               vm.bytecode_len, vm.global_len);
#if DEBUG
  puts("================== DISASM ==================");
#endif

  if (UNLIKELY(a.disassemble)) {
    disassemble(&vm);
    puts("");
  }

  if (UNLIKELY(a.memory_usage)) {
    Stats s = pipeline_allocator.stats(pipeline_allocator.ctx);
    double percent = (s.current * 100) / (double)s.allocated;
    printf("cc: %.2fKB of %.2fKB used (%f%%)\n", s.current / 1024.0,
           s.allocated / 1024.0, percent);
  }

#if DEBUG
  puts("================== MEMORY ==================");
  Stats s = pipeline_allocator.stats(pipeline_allocator.ctx);
  double percent = (s.current * 100) / (double)s.allocated;
  printf("total: %.2fKB of %.2fKB used (%.2f%%)\n", s.current / 1024.0,
         s.allocated / 1024.0, percent);
#endif

  // TODO: fill this with the value of --block-allocator
  Allocator vm_alloc = {0};
  if (a.block_allocator > 0) {
    VERBOSE_PUTS(
        "vm: got --block-allocator, using bump allocator with size %zu",
        a.block_allocator);
    vm_alloc = (Allocator){
        .init = bump_init,
        .request = bump_request,
        .destroy = bump_destroy,
        .reset = bump_reset,
        .stats = bump_stats,
    };
  } else {
    // TODO: init gc here
  }
  int runtime_code = Vm_run(&vm, &vm_alloc);
  VERBOSE_PUTS("vm::Vm_run: executed byte code");

  pipeline_allocator.destroy(pipeline_allocator.ctx);
  VERBOSE_PUTS("mem::Allocator::destroy: Deallocated memory space");

  Vm_destroy(vm);

  VERBOSE_PUTS("vm::Vm_destroy: teared vm down");

  if (a.run == NULL) {
    munmap(input.p, input.len);
  }
  VERBOSE_PUTS("munmap: unmapped input");

  return runtime_code == 0 ? EXIT_SUCCESS : EXIT_FAILURE;
}
