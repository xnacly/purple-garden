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
        -Wno-aggregate-return

FILES := $(shell find . -name "*.c")
.PHONY: run build

run: build
	./purple_garden ./examples/variables.pg

build:
	$(CC) $(FLAGS) $(FILES) -o purple_garden
