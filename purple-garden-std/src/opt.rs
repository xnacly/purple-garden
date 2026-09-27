use purple_garden_runtime::{BoxedType, Type, Vm};

unsafe extern "C" fn some(vm: *mut std::ffi::c_void) {
    let vm = unsafe { &mut *vm.cast::<Vm>() };
    let inner = vm.r(0);
    todo!()
}

pub const PACKAGE: purple_garden_runtime::embed::Pkg = purple_garden_runtime::embed::Pkg {
    name: "opt",
    doc: "package for creating, inspecting and unpacking optionals",
    pkgs: &[],
    fns: &[purple_garden_runtime::embed::Fn {
        name: "some",
        doc: "wrap inner with an optional",
        ptr: some,
        pure: false,
        eval: None,
        arg_names: &["inner"],
        args: &[Type::Slot("T")],
        ret: Type::Option(BoxedType::static_type(&Type::Slot("T"))),
        specialises: None,
    }],
};
