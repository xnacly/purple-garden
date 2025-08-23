CC := gcc
CFLAGS := -std=c23 -Wall -Wextra
# -lm is necessary since pg uses math functions
LDFLAGS := -L../../build -lm -lpg

BIN := embedded
SRC := main.c

all: $(BIN)

$(BIN): $(SRC)
	$(CC) $(CFLAGS) $^ -o $@ $(LDFLAGS)

clean:
	rm -f $(BIN)
