#include <fcntl.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

#include "common.h"
#include "io.h"

Str IO_read_file_to_string(char *path) {
  ASSERT(path != NULL, "path was NULL")

  int fd = open(path, O_RDONLY);
  ASSERT(fd != -1, "failed to read input file")

  struct stat s;
  fstat(fd, &s);
  ASSERT(S_ISREG(s.st_mode), "Path is not a file")

  long length = s.st_size;
  if (length < 0) {
    close(fd);
    return STRING_EMPTY;
  }
  char *buffer = mmap(NULL, length, PROT_READ, MAP_PRIVATE, fd, 0);
  close(fd);

  if (buffer == MAP_FAILED) {
    return STRING_EMPTY;
  }

  close(fd);
  return (Str){.len = length, .p = buffer};
}
