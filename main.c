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

#define CLI_ARGS                                                               \
  X(block_allocator, 'b', (long)0, LONG,                                       \
    "use block allocator with size instead of garbage collection")             \
  X(aot_functions, 'a', false, BOOL, "compile all functions to machine code")  \
  X(disassemble, 'd', false, BOOL,                                             \
    "readable bytecode representation with labels, globals and comments")      \
  X(memory_usage, 'm', false, BOOL,                                            \
    "display the memory usage of parsing, compilation and the virtual "        \
    "machine")                                                                 \
  X(run, 'r', (const char *)(""), STR,                                         \
    "executes the argument as if given inside a file")                         \
  X(verbose, 'V', false, BOOL, "verbose logs")                                 \
  X(stats, 's', false, BOOL, "show statistics")                                \
  X(version, 'v', false, BOOL, "display version information")                  \
  X(gc_max, 0, GC_MIN_HEAP * 64l, LONG,                                        \
    "set hard max gc space in bytes, default is GC_MIN_HEAP*64")               \
  X(gc_size, 0, GC_MIN_HEAP * 2l, LONG, "define gc heap size in bytes")        \
  X(gc_limit, 0, 70.0, DOUBLE,                                                 \
    "instruct memory usage amount for gc to start collecting, in percent "     \
    "(5-99%)")                                                                 \
  X(no_gc, 0, false, BOOL, "disable garbage collection")                       \
  X(no_std, 0, false, BOOL, "limit the standard library to std::len")

typedef struct {
#define X(NAME, SHORT, DEFAULT, TYPE, DESCRIPTION) typeof(DEFAULT) NAME;
  CLI_ARGS
#undef X
  char *filename;
} Args;

void Args_print(Args args) {
  printf("Args{\n");

#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wformat"
#pragma GCC diagnostic ignored "-Wfloat-equal"
#define X(NAME, SHORT, DEFAULT, TYPE, DESCRIPTION)                             \
  do {                                                                         \
    printf("\t%s: ", #NAME);                                                   \
    switch (#TYPE[0]) {                                                        \
    case 'S':                                                                  \
      printf("\"%s\"", args.NAME);                                             \
      break;                                                                   \
    case 'B':                                                                  \
      printf("%s", args.NAME ? "true" : "false");                              \
      break;                                                                   \
    case 'L':                                                                  \
      printf("%ld", (long)args.NAME);                                          \
      break;                                                                   \
    case 'D': /* DOUBLE */                                                     \
      printf("%f", args.NAME);                                                 \
      break;                                                                   \
    default:                                                                   \
      printf("<unknown>");                                                     \
    }                                                                          \
  } while (0);                                                                 \
  puts(",");
  CLI_ARGS
#undef X
#pragma GCC diagnostic pop
  printf("}\n");
}

Args Args_parse(int argc, char **argv) {
  enum {
#define X(NAME, SHORT, DEFAULT, TYPE, DESCRIPTION) __##NAME,
    CLI_ARGS
#undef X
  } __cli_flag;
  SixFlag options[] = {
#define X(NAME, SHORT, DEFAULT, TYPE, DESCRIPTION)                             \
  [__##NAME] = {                                                               \
      .name = #NAME,                                                           \
      .short_name = SHORT,                                                     \
      .type = SIX_##TYPE,                                                      \
      .TYPE = DEFAULT,                                                         \
      .description = DESCRIPTION,                                              \
  },
      CLI_ARGS
#undef X
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

#define X(NAME, SHORT, DEFAULT, TYPE, DESCRIPTION)                             \
  a.NAME = options[__##NAME].TYPE;

  CLI_ARGS
#undef X

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
#if DEBUG
  Args_print(a);
#endif

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

  Gc gc = gc_init(a.gc_size);
  Vm vm = Vm_new(
      (Vm_Config){
          .gc_size = a.gc_size,
          .gc_limit = a.gc_limit,
          .disable_gc = a.no_gc,
          .max_memory = a.gc_max,
          .disable_std = a.no_std,
      },
      pipeline_allocator, &gc);
  gc.vm = &vm;

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
    disassemble(&vm, &ctx, 0, 0, false);
    puts("");
  }

  if (UNLIKELY(a.memory_usage)) {
    Stats s = CALL(pipeline_allocator, stats);
    double percent = (s.current * 100) / (double)s.allocated;
    printf("cc  : %.2fKB of %.2fKB used (%f%%)\n", s.current / 1024.0,
           s.allocated / 1024.0, percent);
  }

  int runtime_code = Vm_run(&vm);
  VERBOSE_PUTS("vm::Vm_run: executed byte code");

  if (UNLIKELY(a.memory_usage)) {
    Stats s = gc_stats(vm.gc);
    double percent = (s.current * 100) / (double)s.allocated;
    printf("vm  : %.2fKB of %.2fKB used (%f%%)\n", s.current / 1024.0,
           s.allocated / 1024.0, percent);
  }

  if (a.stats) {
    bytecode_stats(&vm);
  }

  CALL(pipeline_allocator, destroy);
  free(pipeline_allocator);
  CALL(vm.gc->old, destroy);
  free(vm.gc->old);
  CALL(vm.gc->new, destroy);
  free(vm.gc->new);
  VERBOSE_PUTS("mem::Allocator::destroy: Deallocated memory space");

  if (a.run == NULL) {
    munmap((void *)input.p, input.len);
  }
  VERBOSE_PUTS("munmap: unmapped input");

  return runtime_code == 0 ? EXIT_SUCCESS : EXIT_FAILURE;
}
