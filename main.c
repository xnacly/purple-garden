// TODO: split this up into a DEBUG and a performance entry point
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdbool.h>
#include <string.h>
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
    bool repl;
  int version;
  char *filename;
} Args;

static int run_repl(const Args *a);

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
    __REPL,
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
      [__REPL] = {.name = "repl",
                  .short_name = 'R',
                  .type = SIX_BOOL,
                  .b = false,
                  .description = "start interactive REPL"},
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
  a.disassemble = s.flags[__DISASSEMBLE].b;
  a.memory_usage = s.flags[__MEMORY_USAGE].b;
  a.run = s.flags[__RUN].s;
  a.verbose = s.flags[__VERBOSE].b;
  a.stats = s.flags[__STATS].b;
  a.repl = s.flags[__REPL].b;
  a.version = s.flags[__VERSION].b;

  // command handling
  if (a.version) {
    printf("purple_garden: %s-%s-%s\n", CTX, VERSION, COMMIT);
    if (UNLIKELY(a.verbose)) {
      puts(COMMIT_MSG);
    }
    exit(EXIT_SUCCESS);
  }

  if (!a.repl && a.filename == NULL && (a.run == NULL || a.run[0] == 0)) {
    fprintf(stderr, "error: Missing a file? try `+h/+help` or `+repl`\n");
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

  if (a.repl) {
    return run_repl(&a);
  }

  Str input = {0};
  if (a.run != NULL && a.run[0] != 0) {
    input = (Str){.p = (const uint8_t *)a.run, .len = strlen(a.run)};
  } else {
    input = IO_read_file_to_string(a.filename);
    VERBOSE_PUTS("io::IO_read_file_to_string: mmaped input of size=%zuB",
                 input.len);
  }

#if DEBUG
  Str_debug(&input);
  a.disassemble = true;
  a.memory_usage = true;
  a.stats = true;
#endif

  size_t file_size_or_min = (input.len < MIN_MEM ? MIN_MEM : input.len);
  size_t min_size = (
                        // size for globals
                        (file_size_or_min * sizeof(Value))
                        // size for bytecode
                        + file_size_or_min
                        // size for nodes
                        + (file_size_or_min * sizeof(Node))) /
                    2;

  // this allocator stores both nodes, bytecode and the global pool of the vm,
  // thus it has to life exactly as long as the vm does.
  Allocator *pipeline_allocator = bump_init(min_size);
  VERBOSE_PUTS("mem::init: Allocated memory block of size=%zuB", min_size);
  Lexer l = Lexer_new(input);
  Token **tokens =
      CALL(pipeline_allocator, request, file_size_or_min * sizeof(Token *));
  size_t count = Lexer_all(&l, pipeline_allocator, tokens);
#if DEBUG
  for (size_t i = 0; i < count; i++) {
    Token_debug(tokens[i]);
    puts("");
  }
#endif
  VERBOSE_PUTS("lexer::Lexer_all: lexed tokens count=%zu", count);
  if (UNLIKELY(a.memory_usage)) {
    Stats s = CALL(pipeline_allocator, stats);
    double percent = (s.current * 100) / (double)s.allocated;
    printf("lex : %.2fKB of %.2fKB used (%.2f%%)\n", s.current / 1024.0,
           s.allocated / 1024.0, percent);
  }
  Parser p = Parser_new(pipeline_allocator, tokens);
  size_t node_count = file_size_or_min * sizeof(Node *) / 4;
  Node **nodes = CALL(pipeline_allocator, request, node_count);
  node_count = Parser_all(nodes, &p, node_count);
#if DEBUG
  for (size_t i = 0; i < node_count; i++) {
    Node_debug(nodes[i], 0);
    puts("");
  }
#endif
  VERBOSE_PUTS("parser::Parser_next created AST with node_count=%zu",
               node_count);
  if (UNLIKELY(a.memory_usage)) {
    Stats s = CALL(pipeline_allocator, stats);
    double percent = (s.current * 100) / (double)s.allocated;
    printf("ast : %.2fKB of %.2fKB used (%.2f%%)\n", s.current / 1024.0,
           s.allocated / 1024.0, percent);
  }

  // alloc is NULL here, because we are setting it later on, depending on the
  // cli configuration
  Vm vm = Vm_new(pipeline_allocator, NULL);
  Ctx ctx = cc(&vm, pipeline_allocator, nodes, node_count);
  VERBOSE_PUTS("cc::cc: Flattened AST to byte code/global pool length=%llu/%zu",
               (unsigned long long)vm.bytecode_len, (size_t)vm.global_len);

  if (!a.repl && UNLIKELY(a.disassemble)) {
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
    vm.alloc = bump_init(a.block_allocator * 1024);
  } else {
    vm.alloc = xcgc_init(GC_MIN_HEAP, &vm);
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

static int run_repl(const Args *a) {
  // fixed-size working buffers sized for interactive use
  size_t file_size_or_min = MIN_MEM;
  size_t min_size = ((file_size_or_min * sizeof(Value)) + file_size_or_min +
                     (file_size_or_min * sizeof(Node))) /
                    2;

  Allocator *pipeline_allocator = bump_init(min_size);
  Token **tokens = CALL(pipeline_allocator, request,
                        file_size_or_min * sizeof(Token *));
  size_t node_cap = file_size_or_min * sizeof(Node *) / 4;
  Node **nodes = CALL(pipeline_allocator, request, node_cap);

  Vm vm = Vm_new(pipeline_allocator, NULL);
  if (a->block_allocator > 0) {
    vm.alloc = bump_init(a->block_allocator * 1024);
  } else {
    vm.alloc = xcgc_init(GC_MIN_HEAP, &vm);
  }

  printf("purple_garden REPL. Type :q to quit, :help for help.\n");
  char line[8192];
  Ctx last_ctx = (Ctx){0};
  while (true) {
    printf("> ");
    fflush(stdout);
    if (fgets(line, sizeof(line), stdin) == NULL) {
      break;
    }
    size_t linelen = strlen(line);
    while (linelen && (line[linelen - 1] == '\n' || line[linelen - 1] == '\r')) {
      line[--linelen] = '\0';
    }
    if (linelen == 0) {
      continue;
    }
    if (strcmp(line, ":q") == 0 || strcmp(line, ":quit") == 0) {
      break;
    }
    if (strcmp(line, ":help") == 0) {
      puts(":q/:quit  exit\n:dis      disassemble last submission\n:stats    show bytecode stats\n:mem      show memory usage\n:reset    reset VM state");
      continue;
    }
    if (strcmp(line, ":reset") == 0) {
      Vm_destroy(&vm);
      CALL(pipeline_allocator, destroy);
      free(pipeline_allocator);
      pipeline_allocator = bump_init(min_size);
      tokens = CALL(pipeline_allocator, request,
                    file_size_or_min * sizeof(Token *));
      nodes = CALL(pipeline_allocator, request, node_cap);
      vm = Vm_new(pipeline_allocator, NULL);
      vm.alloc = (a->block_allocator > 0) ? bump_init(a->block_allocator * 1024)
                                          : xcgc_init(GC_MIN_HEAP, &vm);
      memset(&last_ctx, 0, sizeof(last_ctx));
      puts("reset");
      continue;
    }
    if (strcmp(line, ":dis") == 0) {
      disassemble(&vm, &last_ctx);
      puts("");
      continue;
    }
    if (strcmp(line, ":stats") == 0) {
      bytecode_stats(&vm);
      continue;
    }
    if (strcmp(line, ":mem") == 0) {
      Stats s = CALL(vm.alloc, stats);
      double percent = (s.current * 100) / (double)s.allocated;
      printf("vm  : %.2fKB of %.2fKB used (%f%%)\n", s.current / 1024.0,
             s.allocated / 1024.0, percent);
      continue;
    }

    // copy line into allocator and lex/parse
    uint8_t *persist = CALL(pipeline_allocator, request, linelen + 1);
    memcpy(persist, line, linelen);
    persist[linelen] = '\0';
    Str src = (Str){.p = persist, .len = linelen};
    Lexer rl = Lexer_new(src);
    size_t rcount = Lexer_all(&rl, pipeline_allocator, tokens);
    (void)rcount;
    Parser rp = Parser_new(pipeline_allocator, tokens);
    size_t node_count = Parser_all(nodes, &rp, node_cap);

    bool only_function_defs = node_count > 0;
    size_t function_hash = Str_hash(&STRING("function"));
    for (size_t i = 0; i < node_count; i++) {
      Node *tn = nodes[i];
      if (!(tn->type == N_BUILTIN && tn->token &&
            tn->token->string.hash == function_hash)) {
        only_function_defs = false;
        break;
      }
    }

    Ctx new_ctx = cc_seeded(&vm, pipeline_allocator, nodes, node_count,
                            last_ctx.global_hash_buckets ? &last_ctx : NULL);
    last_ctx = new_ctx;

    if (!only_function_defs) {
      int code = Vm_run(&vm);
      if (code != 0) {
        fprintf(stderr, "[repl] runtime error (code=%d)\n", code);
      }
    }
  }

  CALL(pipeline_allocator, destroy);
  free(pipeline_allocator);
  Vm_destroy(&vm);
  return EXIT_SUCCESS;
}
