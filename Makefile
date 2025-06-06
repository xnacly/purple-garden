CC := gcc

FLAGS := -std=c23 \
        -Wall -Wextra -Werror -fdiagnostics-color=always \
        -fno-common -Winit-self -Wfloat-equal -Wundef -Wshadow \
        -Wpointer-arith -Wcast-align -Wstrict-prototypes \
        -Wstrict-overflow=5 -Wwrite-strings -Waggregate-return \
        -Wno-cast-qual -Wswitch-default -Wunreachable-code \
        -Wno-ignored-qualifiers -Wno-unused-parameter \
        -Wno-unused-function -Wno-unused-variable -Wno-aggregate-return \
		-Wno-unused-command-line-argument -lm

RELEASE_FLAGS := -O3 -flto -fno-semantic-interposition \
                 -fno-asynchronous-unwind-tables -march=native

COMMIT := $(shell git rev-parse --short HEAD)
COMMIT_MSG := $(shell git log -1 --pretty=format:'hash=`%H`\nauthor=`%an`\nauthor_email=`%ae`\ncommit_date=`%cd`\ncommit_msg=`%s`')

SRC_DIR := .
TEST_DIR := ./tests
OBJ_DIR := build/cache
BIN_DIR := build

# Define project sources (excluding main and tests)
SRC := $(shell find . -name "*.c" ! -path "./main.c" ! -path "./tests/*")
TEST_SRC := $(shell find ./tests -name "*.c")

# Object paths
SRC_OBJ := $(patsubst %.c,$(OBJ_DIR)/%.o,$(SRC))
TEST_OBJ := $(patsubst %.c,$(OBJ_DIR)/%.o,$(TEST_SRC)) $(SRC_OBJ)
OBJ := $(SRC_OBJ) $(OBJ_DIR)/main.o

# Binaries
DEBUG_BIN := $(BIN_DIR)/purple_garden_debug
VERBOSE_BIN := $(BIN_DIR)/purple_garden_verbose
RELEASE_BIN := $(BIN_DIR)/purple_garden
BENCH_BIN := $(BIN_DIR)/bench
TEST_BIN := $(TEST_DIR)/test

PG := ./examples/hello-world.garden

.PHONY: all run verbose release bench test clean

all: release

# Ensure build dirs
$(OBJ_DIR) $(BIN_DIR):
	mkdir -p $@

# Build .o files from .c
$(OBJ_DIR)/%.o: %.c | $(OBJ_DIR)
	@mkdir -p $(dir $@)
	$(CC) $(FLAGS) -MMD -MP -c $< -o $@

# Final binaries
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
	rm -rf $(BIN_DIR) $(OBJ_DIR) $(TEST_BIN)

# Include generated dependency files if any exist
-include $(wildcard $(OBJ_DIR)/**/*.d) $(wildcard $(OBJ_DIR)/*.d)
