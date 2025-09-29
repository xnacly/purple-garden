#pragma once

#include "common.h"
#include "mem.h"
#include "vm.h"

#define ARG(I) (vm->registers[vm->arg_offset + 1 + (I)])
#define RETURN(...) (vm->registers[0] = (Value)__VA_ARGS__)

void builtin_print(Vm *vm);
void builtin_println(Vm *vm);
void builtin_len(Vm *vm);
void builtin_type(Vm *vm);
void builtin_Some(Vm *vm);
void builtin_None(Vm *vm);
void builtin_assert(Vm *vm);
