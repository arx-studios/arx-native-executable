//! Declarations for the small C runtime shim (`runtime.c`, compiled and
//! linked in at Phase 6's `anx build` step) plus libc's `malloc`/`calloc` —
//! and the codegen-side glue for calling into them.

use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::FunctionValue;
use inkwell::AddressSpace;

pub struct RuntimeFns<'ctx> {
    pub malloc: FunctionValue<'ctx>,
    pub calloc: FunctionValue<'ctx>,
    pub print_int: FunctionValue<'ctx>,
    pub print_float: FunctionValue<'ctx>,
    pub print_bool: FunctionValue<'ctx>,
    pub print_str: FunctionValue<'ctx>,
    // Declared (and implemented in runtime.c) per the runtime-shim spec, but
    // no P0 benchmark ever prints a raw array — codegen never emits a call
    // to it. Kept for when a program does.
    #[allow(dead_code)]
    pub print_array: FunctionValue<'ctx>,
    pub panic_oob: FunctionValue<'ctx>,
    pub panic_div_zero: FunctionValue<'ctx>,
    pub panic_neg_size: FunctionValue<'ctx>,
}

impl<'ctx> RuntimeFns<'ctx> {
    pub fn declare(context: &'ctx Context, module: &Module<'ctx>) -> Self {
        let ptr_ty = context.ptr_type(AddressSpace::default());
        let i64_ty = context.i64_type();
        let f64_ty = context.f64_type();
        let i8_ty = context.i8_type();
        let void_ty = context.void_type();

        let malloc = module.add_function("malloc", ptr_ty.fn_type(&[i64_ty.into()], false), None);
        let calloc = module.add_function(
            "calloc",
            ptr_ty.fn_type(&[i64_ty.into(), i64_ty.into()], false),
            None,
        );
        let print_int = module.add_function(
            "anx_print_int",
            void_ty.fn_type(&[i64_ty.into()], false),
            None,
        );
        let print_float = module.add_function(
            "anx_print_float",
            void_ty.fn_type(&[f64_ty.into()], false),
            None,
        );
        let print_bool = module.add_function(
            "anx_print_bool",
            void_ty.fn_type(&[i8_ty.into()], false),
            None,
        );
        let print_str = module.add_function(
            "anx_print_str",
            void_ty.fn_type(&[ptr_ty.into()], false),
            None,
        );
        let print_array = module.add_function(
            "anx_print_array",
            void_ty.fn_type(&[i64_ty.into(), ptr_ty.into()], false),
            None,
        );
        let panic_oob = module.add_function(
            "anx_panic_oob",
            void_ty.fn_type(&[i64_ty.into(), i64_ty.into()], false),
            None,
        );
        let panic_div_zero =
            module.add_function("anx_panic_div_zero", void_ty.fn_type(&[], false), None);
        let panic_neg_size = module.add_function(
            "anx_panic_neg_size",
            void_ty.fn_type(&[i64_ty.into()], false),
            None,
        );

        RuntimeFns {
            malloc,
            calloc,
            print_int,
            print_float,
            print_bool,
            print_str,
            print_array,
            panic_oob,
            panic_div_zero,
            panic_neg_size,
        }
    }
}
