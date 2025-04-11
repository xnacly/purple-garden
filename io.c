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
  ASSERT(fstat(fd, &s) == 0, "failed to fstat")
  ASSERT(S_ISREG(s.st_mode), "path is not a file")

  long length = s.st_size;
  if (length < 0) {
    close(fd);
    // TODO: should we really assert here?
    ASSERT(length > 0, "input is empty")
  }
  char *buffer = mmap(NULL, length, PROT_READ, MAP_PRIVATE, fd, 0);
  ASSERT(close(fd) == 0, "failed to close file");
  ASSERT(buffer != MAP_FAILED, "failed to mmap input")
  return (Str){.len = length, .p = buffer};
}
