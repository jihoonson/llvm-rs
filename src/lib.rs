//! JIT Compiler to generate code fragments in runtime.

extern crate libc;
extern crate llvm_sys;

#[macro_use]pub mod macros;
pub mod analysis;
pub mod block;
pub mod buffer;
pub mod builder;
pub mod module;
pub mod util;
pub mod types;
pub mod value;

// public reimports from llvm_sys;
pub use llvm_sys::prelude::{LLVMContextRef, LLVMModuleRef, LLVMValueRef};

use std::mem;
use std::ptr;

use llvm_sys::core;
use llvm_sys::prelude::LLVMTypeRef;
use llvm_sys::execution_engine::{LLVMAddGlobalMapping, LLVMAddModule,
                                 LLVMCreateMCJITCompilerForModule, LLVMExecutionEngineRef,
                                 LLVMGetPointerToGlobal, LLVMLinkInMCJIT,
                                 LLVMMCJITCompilerOptions, LLVMRemoveModule};
use llvm_sys::target::{LLVM_InitializeNativeAsmPrinter, LLVM_InitializeNativeTarget};
use llvm_sys::target_machine::LLVMCodeModel;

use libc::{c_char, c_uint};

pub use analysis::Verifier;
pub use block::BasicBlock;
pub use builder::{Builder, CastOp};
pub use module::Module;
pub use types::Ty;
pub use value::{Arg, Function, GlobalValue, Predicate, ToValue, Value, ValueIter, ValueRef};

use types::{FunctionTy, LLVMTy};

pub const JIT_OPT_LVEL: usize = 2;

#[repr(C)]
#[derive(Copy, Clone)]
pub enum AddressSpace {
  Generic = 0,
  Global = 1,
  Shared = 3,
  Const = 4,
  Local = 5,
}

pub trait LLVMRef<T> {
  fn as_ref(&self) -> T;
}

extern "C" {
  pub fn LLVMVersionMajor() -> u32;
  pub fn LLVMVersionMinor() -> u32;
}

fn new_jit_ee(m: &Module, opt_lv: usize) -> Result<LLVMExecutionEngineRef, String> {
  // Transfer its ownership to ExecutionEngine.
  unsafe {
    let mut ee: LLVMExecutionEngineRef = mem::uninitialized();
    let mut err: *mut c_char = mem::uninitialized();

    LLVMLinkInMCJIT();
    expect_noerr!(LLVM_InitializeNativeTarget(),
                  "failed to initialize native target");
    expect_noerr!(LLVM_InitializeNativeAsmPrinter(),
                  "failed to initialize native asm printer");

    let mut opts = new_mcjit_compiler_options(opt_lv);
    let opts_size = mem::size_of::<LLVMMCJITCompilerOptions>();

    let ret = LLVMCreateMCJITCompilerForModule(&mut ee, m.0, &mut opts, opts_size as u64, &mut err);
    llvm_ret!(ret, ee, err)
  }
}

fn new_mcjit_compiler_options(opt_lv: usize) -> LLVMMCJITCompilerOptions {
  LLVMMCJITCompilerOptions {
    OptLevel: opt_lv as c_uint,
    CodeModel: LLVMCodeModel::LLVMCodeModelJITDefault,
    NoFramePointerElim: 0,
    EnableFastISel: 1,
    MCJMM: ptr::null_mut(),
  }
}

pub struct JitCompiler {
  ctx: LLVMContextRef,
  module: Module,
  ee: LLVMExecutionEngineRef,
  builder: Builder,

  void_ty: Ty,
  bool_ty: Ty,
  i8_ty: Ty,
  i16_ty: Ty,
  i32_ty: Ty,
  i64_ty: Ty,
  u64_ty: Ty,
  f32_ty: Ty,
  f64_ty: Ty,
}

impl JitCompiler {
  pub fn new(module_name: &str) -> Result<JitCompiler, String> {
    let ctx = JitCompiler::create_llvm_ctx();
    let module = Module::new(ctx, module_name);
    JitCompiler::new_internal(ctx, module)
  }

  pub fn from_bc(bitcode_path: &str) -> Result<JitCompiler, String> {
    let ctx = JitCompiler::create_llvm_ctx();
    let module = try!(Module::from_bc(ctx, bitcode_path));
    JitCompiler::new_internal(ctx, module)
  }

  pub fn from_module(module: Module) -> Result<JitCompiler, String> {
    JitCompiler::new_internal(JitCompiler::create_llvm_ctx(), module)
  }

  fn create_llvm_ctx() -> LLVMContextRef {
    unsafe { core::LLVMContextCreate() }
  }

  fn new_internal(ctx: LLVMContextRef, mut module: Module) -> Result<JitCompiler, String> {
    module.forget();

    let ee = try!(new_jit_ee(&module, JIT_OPT_LVEL));
    let builder = Builder(unsafe { core::LLVMCreateBuilderInContext(ctx) });

    Ok(JitCompiler {
      ctx: ctx.clone(),
      module: module,
      ee: ee,
      builder: builder,

      void_ty: Ty::void_ty(ctx),
      bool_ty: bool::llvm_ty(ctx),
      i8_ty: i8::llvm_ty(ctx),
      i16_ty: i16::llvm_ty(ctx),
      i32_ty: i32::llvm_ty(ctx),
      i64_ty: i64::llvm_ty(ctx),
      u64_ty: u64::llvm_ty(ctx),
      f32_ty: f32::llvm_ty(ctx),
      f64_ty: f64::llvm_ty(ctx),
    })
  }

  pub fn context(&self) -> LLVMContextRef {
    self.ctx
  }
  pub fn module(&self) -> &Module {
    &self.module
  }
  pub fn engine(&self) -> LLVMExecutionEngineRef {
    self.ee
  }
  pub fn builder(&self) -> &Builder {
    &self.builder
  }

  /// Returns the target data of the base module represented as a string
  pub fn target(&self) -> &str {
    self.module.target()
  }

  /// Get the data layout of the base module
  pub fn data_layout(&self) -> &str {
    self.module.data_layout()
  }

  /// Add a module to the list of modules to interpret or compile.
  pub fn add_module(&self, m: &Module) {
    unsafe { LLVMAddModule(self.ee, m.0) }
  }

  /// Remove a module from the list of modules to interpret or compile.
  pub fn remove_module(&self, m: &Module) -> LLVMModuleRef {
    unsafe {
      let mut out = mem::uninitialized();
      LLVMRemoveModule(self.ee, m.0, &mut out, ptr::null_mut());
      out
    }
  }

  /// Optimize this module with the given optimization level and size level.
  ///
  /// This runs passes depending on the levels given.
  pub fn optimize(&self, opt_level: usize, size_level: usize) {
    self.module.optimize(opt_level, size_level)
  }

  /// Verify that the module is safe to run, returning a string detailing the error
  /// when an error occurs.
  pub fn verify(&self) -> Result<(), String> {
    self.module.verify()
  }

  /// Dump the module to stderr (for debugging).
  pub fn dump(&self) {
    self.module.dump()
  }

  pub fn new_builder(&self) -> Builder {
    Builder::new(self.ctx)
  }

  /// Returns the type with the name given, or `None`` if no type with that name exists.
  pub fn get_ty(&self, name: &str) -> Option<Ty> {
    self.module.get_ty(name)
  }

  /// Make a new pointer with the given element type.
  #[inline(always)]
  pub fn get_pointer_ty(&self, ty: &Ty) -> Ty {
    Ty(unsafe { core::LLVMPointerType(ty.0, 0 as c_uint) })
  }

  pub fn get_void_ty(&self) -> &Ty {
    &self.void_ty
  }
  pub fn get_bool_ty(&self) -> &Ty {
    &self.bool_ty
  }
  pub fn get_i8_ty(&self) -> &Ty {
    &self.i8_ty
  }
  pub fn get_i16_ty(&self) -> &Ty {
    &self.i16_ty
  }
  pub fn get_i32_ty(&self) -> &Ty {
    &self.i32_ty
  }
  pub fn get_i64_ty(&self) -> &Ty {
    &self.i64_ty
  }
  pub fn get_u64_ty(&self) -> &Ty {
    &self.u64_ty
  }
  pub fn get_f32_ty(&self) -> &Ty {
    &self.f32_ty
  }
  pub fn get_f64_ty(&self) -> &Ty {
    &self.f64_ty
  }

  pub fn create_func_ty(ret: &Ty, args: &[&Ty]) -> FunctionTy {
    let ref_array = to_llvmref_array!(args, LLVMTypeRef);

    FunctionTy(unsafe {
      core::LLVMFunctionType(ret.0,
                             ref_array.as_ptr() as *mut LLVMTypeRef,
                             args.len() as c_uint,
                             0)
    })
  }

  pub fn get_const<T: ToValue>(&self, val: T) -> Value {
    val.to_value(self.ctx)
  }

  /// Add an external global to the module with the given type and name.
  pub fn add_global(&self, name: &str, ty: &Ty) -> GlobalValue {
    self.module.add_global(name, ty)
  }

  /// Add a global in the given address space to the module with the given type and name.
  pub fn add_global_in_addr_space(&self, name: &str, ty: &Ty, sp: AddressSpace) -> GlobalValue {
    self.module.add_global_in_addr_space(name, ty, sp)
  }

  /// Add a constant global to the module with the given type, name and value.
  pub fn add_global_constant(&self, name: &str, val: &Value) -> GlobalValue {
    self.module.add_global_constant(name, val)
  }

  /// Get the global with the name given, or `None` if no global with that name exists.
  pub fn get_global(&self, name: &str) -> Option<GlobalValue> {
    self.module.get_global(name)
  }

  /// Get an iterator of global values
  pub fn global_values(&self) -> ValueIter<GlobalValue> {
    self.module.global_values()
  }

  /// Returns a pointer to the global value given.
  ///
  /// This is marked as unsafe because the type cannot be guranteed to be the same as the
  /// type of the global value at this point.
  pub unsafe fn get_ptr_to_global<T>(&self, global: &Value) -> *const T {
    mem::transmute(LLVMGetPointerToGlobal(self.ee, global.0))
  }

  /// Maps a global to a specific memory location.
  pub unsafe fn add_global_mapping<T, V: LLVMRef<LLVMValueRef>>(&self,
                                                                global: &V,
                                                                addr: *const T) {
    LLVMAddGlobalMapping(self.ee, global.as_ref(), mem::transmute(addr));
  }

  /// Add a function to the module with the name given.
  pub fn add_func(&self, name: &str, sig: &FunctionTy) -> Function {
    self.module.add_func(name, sig)
  }

  /// Returns the function with the name given, or `None` if no function with that name exists.
  pub fn get_func(&self, name: &str) -> Option<Function> {
    self.module.get_func(name)
  }

  /// Returns the function after creating prototype and initialize the entry block
  pub fn create_func_prototype(&self,
                               name: &str,
                               ret_ty: &Ty,
                               param_tys: &[&Ty],
                               builder: Option<&Builder>)
                               -> Function {
    let func_ty = JitCompiler::create_func_ty(ret_ty, param_tys);
    let func = self.add_func(name, &func_ty);

    if let Some(b) = builder {
      let entry = func.append("entry");
      b.position_at_end(&entry)
    }

    func
  }

  /// Returns a pointer to the machine code for the raw function poionter.
  ///
  /// This is marked as unsafe because the defined function signature and
  /// return could be different from their internal representation.
  pub unsafe fn get_func_ptr(&self, func: &Function) -> Option<*const ()> {
    let ptr: *const u8 = self.get_ptr_to_global(&func.into());
    Some(mem::transmute(ptr))
  }

  pub fn delete_func(&self, func: &Function) {
    unsafe { core::LLVMDeleteFunction(func.0) }
  }
}

impl Drop for JitCompiler {
  fn drop(&mut self) {
    unsafe {
      core::LLVMContextDispose(self.ctx);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use libc::c_void;
  use types::LLVMTy;

  pub extern "C" fn test_extern_fn(x: u64) -> u64 {
    x
  }

  #[test]
  fn test_modules() {
    let jit = JitCompiler::new("test_jit").ok().unwrap();
    let ctx = jit.context();

    let module1 = Module::new(jit.context(), "internal");

    let bld = &jit.new_builder();
    let func_ty = &JitCompiler::create_func_ty(&u64::llvm_ty(ctx), &[&u64::llvm_ty(ctx)]);
    let func = module1.add_func("test1", func_ty);

    let entry = func.append("entry");
    bld.position_at_end(&entry);
    bld.create_ret(&func.arg(0).into());
    func.dump();
    func.verify().ok().expect("Function test is invalid");

    assert!(jit.get_func("test1").is_none());
    assert!(module1.get_func("test1").is_some());

    let found_func = module1.get_func("test1").expect("test1 not found");
    let found_fn_ptr_1 = unsafe { jit.get_func_ptr(&found_func).expect("test1 ptr not found (try 1)") };
    assert!(found_fn_ptr_1.is_null() == true);

    jit.add_module(&module1);
    let found_fn_ptr_2 = unsafe { jit.get_func_ptr(&found_func).expect("test1 ptr not found (try 2)") };
    assert!(found_fn_ptr_2.is_null() == false);

    let equal_fn: fn(u64) -> u64 = unsafe { ::std::mem::transmute(found_fn_ptr_2) };
    assert_eq!(19800401, equal_fn(19800401));

    jit.remove_module(&module1);
    let found_fn_ptr_3 = unsafe { jit.get_func_ptr(&found_func).expect("test1 ptr not found (try 3)") };
    assert!(found_fn_ptr_3.is_null() == true);

    jit.add_module(&module1);
    let found_fn_ptr_4 = unsafe { jit.get_func_ptr(&found_func).expect("test1 ptr not found (try 4)") };
    assert!(found_fn_ptr_4.is_null() == false);
  }

  #[test]
  fn test_global_mapping() {
    let jit = JitCompiler::new("test_jit").ok().unwrap();
    let ctx = jit.context();

    let func = jit.create_func_prototype("test", &u64::llvm_ty(ctx), &[&u64::llvm_ty(ctx)], None);
    let fn_ptr: *const c_void = unsafe { ::std::mem::transmute(test_extern_fn) };
    unsafe { jit.add_global_mapping(&func, fn_ptr) };

    println!("before transmute");
    let same: fn(u64) -> u64 = unsafe { ::std::mem::transmute(jit.get_func_ptr(&func).unwrap()) };
    println!("after transmute");

    for i in 0..10 {
      assert_eq!(i, same(i));
    }

    println!("after execute same");

    jit.verify().unwrap();
    println!("after verify");
  }

  #[test]
  fn test_version() {
    assert!(unsafe { LLVMVersionMajor() } >= 3);
    assert!(unsafe { LLVMVersionMinor() } >= 6);
  }
}
