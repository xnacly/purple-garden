use purple_garden_runtime::Vm;

pub fn println(vm: &mut Vm) {
    println!("{}", vm.r(0).as_str(&vm.strings, &vm.string_data));
}

pub fn print(vm: &mut Vm) {
    print!("{}", vm.r(0).as_str(&vm.strings, &vm.string_data));
}
