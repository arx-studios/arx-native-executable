//! Declarations for the small C runtime shim (`runtime.c`, compiled and
//! linked in at Phase 6's `anx build` step) plus libc's `malloc`/`calloc` —
//! and the codegen-side glue for calling into them.

use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::StructType;
use inkwell::values::FunctionValue;
use inkwell::AddressSpace;

/// The `{ i64 length, ptr data }` layout shared by strings and arrays (see
/// `Codegen::len_data_struct_type` in mod.rs) — duplicated here since
/// `RuntimeFns::declare` only has a bare `Context`, not a `Codegen`. Struct
/// params/returns across this C boundary rely on the same by-value struct
/// ABI lowering already proven for array parameters passed between ANX
/// functions (see docs/P2/ANX-P2-Strings-Plan-v1.md Step 3).
fn str_struct_type<'ctx>(context: &'ctx Context) -> StructType<'ctx> {
    let ptr_ty = context.ptr_type(AddressSpace::default());
    context.struct_type(&[context.i64_type().into(), ptr_ty.into()], false)
}

pub struct RuntimeFns<'ctx> {
    pub malloc: FunctionValue<'ctx>,
    pub calloc: FunctionValue<'ctx>,
    pub print_int: FunctionValue<'ctx>,
    pub print_float: FunctionValue<'ctx>,
    pub print_bool: FunctionValue<'ctx>,
    pub print_str: FunctionValue<'ctx>,
    pub print_array: FunctionValue<'ctx>,
    pub panic_oob: FunctionValue<'ctx>,
    pub panic_div_zero: FunctionValue<'ctx>,
    pub panic_neg_size: FunctionValue<'ctx>,
    pub str_concat: FunctionValue<'ctx>,
    pub str_char_at: FunctionValue<'ctx>,
    pub str_substring: FunctionValue<'ctx>,
    pub str_equals: FunctionValue<'ctx>,
    pub panic_str_oob: FunctionValue<'ctx>,
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

        let str_ty = str_struct_type(context);
        let str_concat = module.add_function(
            "anx_str_concat",
            str_ty.fn_type(&[str_ty.into(), str_ty.into()], false),
            None,
        );
        let str_char_at = module.add_function(
            "anx_str_char_at",
            str_ty.fn_type(&[str_ty.into(), i64_ty.into()], false),
            None,
        );
        let str_substring = module.add_function(
            "anx_str_substring",
            str_ty.fn_type(&[str_ty.into(), i64_ty.into(), i64_ty.into()], false),
            None,
        );
        // Returns i8, not i1 — crossing the C ABI boundary as a byte matches
        // the existing `anx_print_bool` convention (see codegen_print).
        let str_equals = module.add_function(
            "anx_str_equals",
            i8_ty.fn_type(&[str_ty.into(), str_ty.into()], false),
            None,
        );
        let panic_str_oob = module.add_function(
            "anx_panic_str_oob",
            void_ty.fn_type(&[i64_ty.into(), i64_ty.into()], false),
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
            str_concat,
            str_char_at,
            str_substring,
            str_equals,
            panic_str_oob,
        }
    }
}
