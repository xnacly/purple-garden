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
FILES := $(shell find . -name "*.c")
PG := ./examples/hello-world.garden
.PHONY: run build

run: build
	./purple_garden $(PG)

build:
	$(CC) $(FLAGS) -DCOMMIT='"$(COMMIT)"' $(FILES) -o purple_garden
