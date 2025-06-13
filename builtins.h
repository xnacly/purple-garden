#pragma once

#include "common.h"
#include "mem.h"
#include "vm.h"

void builtin_print(Vm *vm);
void builtin_println(Vm *vm);
void builtin_len(Vm *vm);
void builtin_type(Vm *vm);
void builtin_Some(Vm *vm);
