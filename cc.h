#ifndef PGCC_H
#define PGCC_H

#include "vm.h"

// cc consumes a Node and its children to populate a Vm, its global pool, its
// bytecode and do all prep the runtime requires
Vm cc(Node *n);

// disassemble produces a readable bytecode representation with labels, globals
// and comments as a heap allocated string
String disassemble(const Vm *vm);

#endif
