#ifndef IO_H
#define IO_H

#include "common.h"

// Read file at path to Str, resulting Str.p has to be deallocated with
// munmap
Str IO_read_file_to_string(char *path);

#endif
