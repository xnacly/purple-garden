#include "../vm.h"
#include <fcntl.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

#define MAX_PATH 256

static void builtin_fs_read_file(Vm *vm) {
  Value path_value = ARG(0);
  if (path_value.string->len == 0)
    goto invalid;

  const Str *path = path_value.string;
  if (path->len > MAX_PATH)
    goto invalid;

  // fuck unix, because every c api requires 0 termination
  char path_buf[MAX_PATH + 1];
  memcpy(path_buf, path->p, path->len);
  path_buf[path->len] = '\0';

  int fd = open(path_buf, O_RDONLY);
  if (fd == -1)
    goto invalid;

  struct stat s;
  if (fstat(fd, &s) == -1 || !S_ISREG(s.st_mode)) {
    close(fd);
    goto invalid;
  }

  size_t length = s.st_size;
  if (length == 0) {
    close(fd);
    goto invalid;
  }

  char *buf = gc_request(&vm->gc, length, GC_OBJ_STR);
  ssize_t r = read(fd, buf, length);
  close(fd);
  if (r < 0) {
    goto invalid;
  }

  Str *sbuf = gc_request(&vm->gc, sizeof(Str), GC_OBJ_STR);
  *sbuf = (Str){.len = length, .p = (const uint8_t *)buf};

  RETURN((Value){.type = V_STR, .string = sbuf, .is_some = true});
  return;

invalid:
  RETURN(*INTERNED_NONE);
}

static void builtin_fs_write_file(Vm *vm) {
  Value path_value = ARG(0);
  Value content_value = ARG(1);

  if (path_value.type != V_STR || content_value.type != V_STR)
    goto invalid;

  const Str *path = path_value.string;
  const Str *content = content_value.string;
  if (path->len == 0 || path->len > MAX_PATH)
    goto invalid;

  char path_buf[MAX_PATH + 1];
  memcpy(path_buf, path->p, path->len);
  path_buf[path->len] = '\0';

  int fd = open(path_buf, O_WRONLY | O_CREAT | O_TRUNC, 0644);
  if (fd < 0)
    goto invalid;

  ssize_t written = write(fd, content->p, content->len);
  close(fd);

  if (written != (ssize_t)content->len)
    goto invalid;

  RETURN(*INTERNED_NONE);
  return;

invalid:
  RETURN(*INTERNED_NONE);
}
