#ifndef IO_H
#define IO_H

#include "common.h"

// Read file at path to String, resulting String.p has to be deallocated with
// munmap
String IO_read_file_to_string(char *path);

#endif
