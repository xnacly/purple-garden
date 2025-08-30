// TODO: use pg.h as the debug entry point too.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <sys/time.h>

#include "6cl/6cl.h"
#include "cc.h"
#include "commit.h"
#include "common.h"
#include "io.h"
#include "lexer.h"
#include "mem.h"
#include "parser.h"
#include "strings.h"
#include "vm.h"

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
  size_t block_allocator;
  bool aot_functions;
  bool disassemble;
  bool memory_usage;
  const char *run;
  bool verbose;
  bool stats;
  int version;
  char *filename;
} Args;

Args Args_parse(int argc, char **argv) {
  enum {
    __VERSION,
    __DISASSEMBLE,
    __BLOCK_ALLOC,
    __AOT,
    __MEMORY_USAGE,
    __VERBOSE,
    __STATS,
    __RUN,
  };

  SixFlag options[] = {
      [__VERSION] = {.name = "version",
                     .type = SIX_BOOL,
                     .b = false,
                     .short_name = 'v',
                     .description = "display version information"},
      [__DISASSEMBLE] =
          {.name = "disassemble",
           .short_name = 'd',
           .type = SIX_BOOL,
           .b = false,
           .description =
               "readable bytecode representation with labels, globals "
               "and comments"},
      [__BLOCK_ALLOC] =
          {.name = "block-allocator",
           .short_name = 'b',
           .type = SIX_LONG,
           .description =
               "use block allocator with size instead of garbage collection"},
      [__AOT] = {.name = "aot-functions",
                 .short_name = 'a',
                 .b = false,
                 .type = SIX_BOOL,
                 .description = "compile all functions to machine code"},
      [__MEMORY_USAGE] = {.name = "memory-usage",
                          .short_name = 'm',
                          .b = false,
                          .type = SIX_BOOL,
                          .description = "display the memory usage of parsing, "
                                         "compilation and the virtual "
                                         "machine"},
      [__VERBOSE] = {.name = "verbose",
                     .short_name = 'V',
                     .b = false,
                     .type = SIX_BOOL,
                     .description = "verbose logging"},
      [__STATS] = {.name = "stats",
                   .short_name = 's',
                   .b = false,
                   .type = SIX_BOOL,
                   .description = "show statistics"},
      [__RUN] = {.name = "run",
                 .short_name = 'r',
                 .s = "",
                 .type = SIX_STR,
                 .description =
                     "executes the argument as if an input file was given"},
  };
  Args a = (Args){0};
  Six s = {
      .flags = options,
      .flag_count = sizeof(options) / sizeof(options[0]),
      .name_for_rest_arguments = "<file.garden>",
  };
  SixParse(&s, argc, argv);
  if (s.rest_count) {
    a.filename = s.rest[0];
  }
  a.block_allocator = s.flags[__BLOCK_ALLOC].l;
  a.aot_functions = s.flags[__AOT].b;
  // a.disassemble = s.flags[__DISASSEMBLE].b;
  a.disassemble = s.flags[__DISASSEMBLE].b;
  a.memory_usage = s.flags[__MEMORY_USAGE].b;
  a.run = s.flags[__RUN].s;
  a.verbose = s.flags[__VERBOSE].b;
  a.stats = s.flags[__STATS].b;
  a.version = s.flags[__VERSION].b;

  // command handling
  if (a.version) {
    printf("purple_garden: %s-%s-%s\n", CTX, VERSION, COMMIT);
    if (UNLIKELY(a.verbose)) {
      puts(COMMIT_MSG);
    }
    exit(EXIT_SUCCESS);
  }

  if (a.filename == NULL && (a.run == NULL || a.run[0] == 0)) {
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

  Str input = STRING_EMPTY;
  if (a.run != NULL && a.run[0] != 0) {
    input = (Str){.p = (const uint8_t *)a.run, .len = strlen(a.run), .hash = 0};
  } else {
    input = IO_read_file_to_string(a.filename);
    VERBOSE_PUTS("io::IO_read_file_to_string: mmaped input of size=%uB",
                 input.len);
  }

#if DEBUG
  Str_debug(&input);
  a.disassemble = true;
  a.memory_usage = true;
  a.stats = true;
#endif

  // this allocator stores both nodes, bytecode and the global pool of the vm,
  // thus it has to life exactly as long as the vm does.
  //
  Allocator *pipeline_allocator = bump_init(MIN_MEM, 0);
  VERBOSE_PUTS("mem::init: Allocated memory block of size=%zuB", MIN_MEM);
  Lexer lexer = Lexer_new(input);
  Parser parser = Parser_new(pipeline_allocator, &lexer);

  // TODO: move this into parser::advance
  // VERBOSE_PUTS("lexer::Lexer_all: lexed tokens count=%zu (%zuB)", count,
  // count * sizeof(Token *));
  //
  // TODO: move this into cc::cc (replacing node iteration with something like
  // parser::next) VERBOSE_PUTS("parser::Parser_next created AST with
  // node_count=%zu", node_count);

  // alloc is NULL here, because we are setting it later on, depending on the
  // cli configuration
  Vm vm = Vm_new((Vm_Config){}, pipeline_allocator, NULL);
  if (UNLIKELY(a.memory_usage)) {
    Stats s = CALL(pipeline_allocator, stats);
    double percent = (s.current * 100) / (double)s.allocated;
    printf("vmnew: %.2fKB of %.2fKB used (%f%%)\n", s.current / 1024.0,
           s.allocated / 1024.0, percent);
  }
  Ctx ctx = cc(&vm, pipeline_allocator, &parser);
  VERBOSE_PUTS("cc::cc: Flattened AST to byte code/global pool length=%zu/%zu "
               "(%zuB/%zuB)",
               (size_t)vm.bytecode_len, (size_t)vm.global_len,
               (size_t)vm.bytecode_len * sizeof(uint32_t),
               (size_t)vm.global_len * sizeof(Value));

  if (UNLIKELY(a.disassemble)) {
    disassemble(&vm, &ctx);
    puts("");
  }

  if (UNLIKELY(a.memory_usage)) {
    Stats s = CALL(pipeline_allocator, stats);
    double percent = (s.current * 100) / (double)s.allocated;
    printf("cc  : %.2fKB of %.2fKB used (%f%%)\n", s.current / 1024.0,
           s.allocated / 1024.0, percent);
  }

  if (a.block_allocator > 0) {
    VERBOSE_PUTS(
        "vm: got --block-allocator, using bump allocator with size %zuB/%zuKB",
        a.block_allocator * 1024, a.block_allocator);
    vm.alloc = bump_init(a.block_allocator * 1024, 0);
  } else {
    vm.alloc = xcgc_init(&vm, GC_MIN_HEAP, 0);
  }
  int runtime_code = Vm_run(&vm);
  VERBOSE_PUTS("vm::Vm_run: executed byte code");

  if (UNLIKELY(a.memory_usage)) {
    Stats s = CALL(vm.alloc, stats);
    double percent = (s.current * 100) / (double)s.allocated;
    printf("vm  : %.2fKB of %.2fKB used (%f%%)\n", s.current / 1024.0,
           s.allocated / 1024.0, percent);
  }

  if (a.stats) {
    bytecode_stats(&vm);
  }

  CALL(pipeline_allocator, destroy);
  free(pipeline_allocator);
  VERBOSE_PUTS("mem::Allocator::destroy: Deallocated memory space");

  Vm_destroy(&vm);
  VERBOSE_PUTS("vm::Vm_destroy: teared vm down");

  if (a.run == NULL) {
    munmap((void *)input.p, input.len);
  }
  VERBOSE_PUTS("munmap: unmapped input");

  return runtime_code == 0 ? EXIT_SUCCESS : EXIT_FAILURE;
}
