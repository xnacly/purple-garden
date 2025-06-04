CC := gcc

FLAGS := -std=c23 \
        -lm \
        -Wall \
        -Wextra \
        -Werror \
        -fdiagnostics-color=always \
        -fno-common \
        -Winit-self \
        -Wfloat-equal \
        -Wundef \
        -Wshadow \
        -Wpointer-arith \
        -Wcast-align \
        -Wstrict-prototypes \
        -Wstrict-overflow=5 \
        -Wwrite-strings \
        -Waggregate-return \
        -Wcast-qual \
        -Wswitch-default \
        -Wunreachable-code \
        -Wno-discarded-qualifiers \
        -Wno-unused-parameter \
        -Wno-unused-function \
        -Wno-unused-variable \
        -Wno-aggregate-return

RELEASE_FLAGS := -O3 \
        -flto \
        -fno-semantic-interposition \
        -fno-asynchronous-unwind-tables \
        -march=native

COMMIT := $(shell git rev-parse --short HEAD)
COMMIT_MSG := $(shell git log -1 --pretty=format:'hash=`%H`\nauthor=`%an`\nauthor_email=`%ae`\ncommit_date=`%cd`\ncommit_msg=`%s`')

# Find source files, excluding main.c and test.c
FILES := $(shell find . -name "*.c" ! -name "main.c" ! -path "./tests/test.c")
TEST_FILES := $(shell find ./tests -name "*.c")

OBJ_DIR := build/cache
BIN_DIR := build

# Normalize paths by removing leading "./"
FILES := $(patsubst ./%,%,$(FILES))
TEST_FILES := $(patsubst ./%,%,$(TEST_FILES))

# Combine source files explicitly with main.c (no ./ prefix)
SRC := $(FILES) main.c

# Create object file list
OBJ := $(patsubst %.c,$(OBJ_DIR)/%.o,$(SRC))
OBJ := $(sort $(OBJ)) # remove duplicates and sort

# Test objects include test sources plus main OBJ files
TEST_OBJ := $(patsubst %.c,$(OBJ_DIR)/%.o,$(TEST_FILES)) $(OBJ)
TEST_OBJ := $(sort $(TEST_OBJ))

DEBUG_BIN := $(BIN_DIR)/purple_garden_debug
VERBOSE_BIN := $(BIN_DIR)/purple_garden_verbose
RELEASE_BIN := $(BIN_DIR)/purple_garden
BENCH_BIN := $(BIN_DIR)/bench
TEST_BIN := ./tests/test

PG := ./examples/hello-world.garden

.PHONY: all run verbose release bench test clean

all: release

# Pattern rule for building object files with dependency generation
$(OBJ_DIR)/%.o: %.c | $(OBJ_DIR)
	@mkdir -p $(dir $@)
	$(CC) $(FLAGS) -MMD -MP -c $< -o $@

# Create directories if missing
$(OBJ_DIR):
	mkdir -p $(OBJ_DIR)

$(BIN_DIR):
	mkdir -p $(BIN_DIR)

# Link executables

$(DEBUG_BIN): $(OBJ) | $(BIN_DIR)
	$(CC) -g3 $(FLAGS) -fsanitize=address,undefined -DDEBUG=1 $^ -o $@

$(VERBOSE_BIN): $(OBJ) | $(BIN_DIR)
	$(CC) -g3 $(FLAGS) $(RELEASE_FLAGS) $^ -o $@

$(RELEASE_BIN): $(OBJ) | $(BIN_DIR)
	$(CC) -g3 $(FLAGS) $(RELEASE_FLAGS) -DCOMMIT='"$(COMMIT)"' -DCOMMIT_MSG='"$(COMMIT_MSG)"' $^ -o $@

$(BENCH_BIN): $(OBJ) | $(BIN_DIR)
	$(CC) $(FLAGS) $(RELEASE_FLAGS) -DCOMMIT='"BENCH"' $^ -o $@

$(TEST_BIN): $(TEST_OBJ) | $(BIN_DIR)
	$(CC) $(FLAGS) -g3 -fsanitize=address,undefined -DDEBUG=0 $^ -o $@

# Run targets

run: $(DEBUG_BIN)
	./$(DEBUG_BIN) $(PG)

verbose: $(VERBOSE_BIN)
	./$(VERBOSE_BIN) +V $(PG)

release: $(RELEASE_BIN)

bench: $(BENCH_BIN)
	./$(BENCH_BIN) +V $(PG)

test: $(TEST_BIN)
	./$(TEST_BIN)

clean:
	rm -rf $(BIN_DIR) $(OBJ_DIR) ./tests/test

# Include dependency files
-include $(OBJ_DIR)/**/*.d
-include $(OBJ_DIR)/*.d
