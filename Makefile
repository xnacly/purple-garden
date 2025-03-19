FLAGS := -std=c11 \
        -O2 \
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
        -Wswitch-enum \
        -Wunreachable-code \
        -Wno-discarded-qualifiers \
        -Wno-aggregate-return

FILES := $(shell find . -name "*.c")
.PHONY: run build

run: build
	./purple_garden

build:
	$(CC) $(FLAGS) $(FILES) -o purple_garden
