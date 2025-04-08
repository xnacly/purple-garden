#ifndef PGCC_H
#define PGCC_H

#include "vm.h"

// cc requests a Node from parser::Parser_next compiles said Node and its
// children to populate the Vm, its global pool, its bytecode and do all prep
// the runtime requires
Vm cc(Parser *p);

// disassemble prints a readable bytecode representation with labels, globals
// and comments as a heap allocated string
void disassemble(const Vm *vm);

#endif
