CC ?= gcc
FLAGS := -std=c23 \
        -Wall -Wextra -Werror -fdiagnostics-color \
        -fno-common -Winit-self -Wfloat-equal -Wundef -Wshadow \
        -Wpointer-arith -Wcast-align -Wstrict-prototypes \
        -Wstrict-overflow=5 -Wwrite-strings -Waggregate-return \
        -Wno-cast-qual -Wswitch-default -Wunreachable-code \
        -Wno-ignored-qualifiers -Wno-unused-parameter \
        -Wno-unused-function -Wno-unused-variable -Wno-aggregate-return \
        -Wno-override-init \
		-Wno-unused-command-line-argument -lm

UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
	FLAGS += -DPLATFORM_DARWIN
endif

RELEASE_FLAGS := -O3 -flto -flto=auto -fno-semantic-interposition \
                 -fno-asynchronous-unwind-tables -march=native
# -g3 if you want to run valgrind on the release binary

COMMIT := $(shell git rev-parse --short HEAD)
COMMIT_MSG := $(shell git log -1 --pretty=format:'hash=`%H`\nauthor=`%an`\nauthor_email=`%ae`\ncommit_date=`%cd`\ncommit_msg=`%s`')

SRC_DIR := .
TEST_DIR := ./tests
OBJ_DIR := build/cache
BIN_DIR := build

SRC := $(shell find . -name "*.c" ! -path "./main.c" ! -path "./tests/*" ! -path "./examples/*")
TEST_SRC := $(shell find ./tests -name "*.c")

SRC_OBJ := $(patsubst %.c,$(OBJ_DIR)/%.o,$(SRC))
TEST_OBJ := $(patsubst %.c,$(OBJ_DIR)/%.o,$(TEST_SRC)) $(SRC_OBJ)
OBJ := $(SRC_OBJ) $(OBJ_DIR)/main.o

DEBUG_BIN := $(BIN_DIR)/purple_garden_debug
VERBOSE_BIN := $(BIN_DIR)/purple_garden_verbose
RELEASE_BIN := $(BIN_DIR)/purple_garden
BENCH_BIN := $(BIN_DIR)/bench
TEST_BIN := $(TEST_DIR)/test

PG := ./examples/hello-world.garden

.PHONY: all run verbose release bench test clean lib

all: release

DEBUG_EXTRA := -DDEBUG=1 -fsanitize=address,undefined -g3
TEST_EXTRA := -fsanitize=address,undefined -g3
RELEASE_EXTRA := -DCOMMIT='"$(COMMIT)"' -DCOMMIT_MSG='"$(COMMIT_MSG)"'
BENCH_EXTRA := -DCOMMIT='"BENCH"'

$(OBJ_DIR) $(BIN_DIR):
	mkdir -p $@

# Object compilation uses COMPILE_FLAGS
$(OBJ_DIR)/%.o: %.c | $(OBJ_DIR)
	@mkdir -p $(dir $@)
	$(CC) $(FLAGS) $(COMPILE_FLAGS) -MMD -MP -c $< -o $@

# Debug build
$(DEBUG_BIN): COMPILE_FLAGS := $(DEBUG_EXTRA)
$(DEBUG_BIN): LINK_FLAGS := $(DEBUG_EXTRA)
$(DEBUG_BIN): $(OBJ) | $(BIN_DIR)
	$(CC) $(FLAGS) $(LINK_FLAGS) $^ -o $@

# Release build
$(RELEASE_BIN): COMPILE_FLAGS := $(RELEASE_FLAGS)
$(RELEASE_BIN): LINK_FLAGS := $(RELEASE_FLAGS) $(RELEASE_EXTRA)
$(RELEASE_BIN): $(OBJ) | $(BIN_DIR)
	$(CC) $(FLAGS) $(LINK_FLAGS) $^ -o $@

# Bench build
$(BENCH_BIN): COMPILE_FLAGS := $(RELEASE_FLAGS)
$(BENCH_BIN): LINK_FLAGS := $(RELEASE_FLAGS) $(BENCH_EXTRA)
$(BENCH_BIN): $(OBJ) | $(BIN_DIR)
	$(CC) $(FLAGS) $(LINK_FLAGS) $^ -o $@

$(TEST_BIN): COMPILE_FLAGS := $(TEST_EXTRA)
$(TEST_BIN): LINK_FLAGS := $(TEST_EXTRA)
$(TEST_BIN): $(TEST_OBJ) | $(BIN_DIR)
	$(CC) $(FLAGS) $(LINK_FLAGS) $^ -o $@

# lib build
$(BIN_DIR)/libpg.a: $(SRC_OBJ) | $(BIN_DIR)
	ar rcs $@ $^

# Run targets
run: $(DEBUG_BIN)
	./$(DEBUG_BIN) $(PG)

verbose: $(VERBOSE_BIN)
	./$(VERBOSE_BIN) +V $(PG)

release: $(RELEASE_BIN)

bench: $(BENCH_BIN)
	./$(BENCH_BIN) +V $(PG)

profile: $(RELEASE_BIN)
	rm -fr *.data
	perf record --call-graph dwarf ./build/purple_garden examples/bench.garden
	hotspot

test: $(TEST_BIN)
	./$(TEST_BIN)

lib: $(BIN_DIR)/libpg.a

clean:
	rm -rf $(BIN_DIR) $(OBJ_DIR) $(TEST_BIN)

-include $(wildcard $(OBJ_DIR)/**/*.d) $(wildcard $(OBJ_DIR)/*.d)
