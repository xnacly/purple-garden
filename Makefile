FLAGS := -std=c23 \
        -O3 \
		-flto \
		-fno-semantic-interposition \
		-fno-asynchronous-unwind-tables \
		-march=native \
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
		-Wno-aggregate-return

COMMIT := $(shell git rev-parse --short HEAD)
COMMIT_MSG := $(shell git log -1 --pretty=format:'hash=`%H`\nauthor=`%an`\nauthor_email=`%ae`\ncommit_date=`%cd`\ncommit_msg=`%s`')
FILES := $(shell find . -maxdepth 1 -name "*.c" ! -name "main.c")
TEST_FILES := $(shell find ./tests -name "*.c")
PG := ./examples/hello-world.garden

.PHONY: run build test clean bench

run:
	$(CC) -g3 $(FLAGS) -fsanitize=address,undefined -DDEBUG=1 $(FILES) ./main.c -o purple_garden_debug
	./purple_garden_debug $(PG)

release:
	$(CC) $(FLAGS) -DCOMMIT='"$(COMMIT)"' -DCOMMIT_MSG='"$(COMMIT_MSG)"' $(FILES) ./main.c -o purple_garden

bench:
	$(CC) $(FLAGS) -DCOMMIT='"BENCH"' $(FILES) ./main.c -o bench
	./bench -V $(PG)

test:
	$(CC) $(FLAGS) -g3 -fsanitize=address,undefined -DDEBUG=0 $(TEST_FILES) $(FILES) -o ./tests/test
	./tests/test

clean:
	rm -fv ./purple_garden ./purple_garden_debug ./tests/test ./bench 
