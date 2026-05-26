crate::builtin! {
    pub fn println(vm) {
        println!("{}", vm.r(0).as_str(&vm.strings, &vm.string_data));
    }

    pub fn print(vm) {
        print!("{}", vm.r(0).as_str(&vm.strings, &vm.string_data));
    }
}
