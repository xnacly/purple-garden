
#include <stdlib.h>
#include <sys/stat.h>

#include "common.h"
#include "io.h"

String IO_read_file_to_string(char *path) {
  ASSERT(path != NULL, "path was NULL")

  FILE *file = fopen(path, "rb");
  ASSERT(file != NULL, "failed to read input file")

  struct stat s;
  fstat(file->_fileno, &s);
  // s.st_mode & S_IFREG because somehow this does not compile?
  ASSERT(s.st_mode & 0100000, "Path is not a file")

  long length = s.st_size;
  if (length < 0) {
    fclose(file);
    return STRING_EMPTY;
  }

  char *buffer = malloc(length + 1);
  if (!buffer) {
    fclose(file);
    return STRING_EMPTY;
  }

  size_t read = fread(buffer, 1, length, file);
  ASSERT(read == (size_t)length,
         "Wasnt able to read all bytes from file with fread")
  buffer[length] = '\0';

  fclose(file);
  return (String){.len = length, .p = buffer};
}
