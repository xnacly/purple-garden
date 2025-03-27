FLAGS := -std=c2x \
        -g3 \
        -O3 \
        -Wall \
        -Wextra \
        -Werror \
        -fdiagnostics-color=always \
        -fsanitize=address,undefined \
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
		-Wno-unused-parameter\
		-Wno-aggregate-return

COMMIT := $(shell git rev-parse --short HEAD)
FILES := $(shell find . -maxdepth 1 -name "*.c" ! -name "main.c")
TEST_FILES := $(shell find ./tests -name "*.c")
PG := ./examples/hello-world.garden

.PHONY: run build test clean

run: build
	./purple_garden $(PG)

test:
	$(CC) $(FLAGS) $(TEST_FILES) $(FILES) -DDEBUG=1 -o ./tests/test
	./tests/test

build:
	$(CC) $(FLAGS) -DCOMMIT='"$(COMMIT)"' $(FILES) ./main.c -o purple_garden

clean:
	rm -f purple_garden test/cc_test
