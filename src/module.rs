use std::mem;
use libc::{c_char, c_uint};

use llvm_sys::bit_reader::LLVMParseBitcodeInContext;
use llvm_sys::core;
use llvm_sys::linker;
use llvm_sys::prelude::{LLVMContextRef, LLVMModuleRef};
use llvm_sys::transforms::pass_manager_builder as pass;

use super::{AddressSpace, LLVMRef};
use buffer::MemoryBuffer;
use analysis::Verifier;
use value::{Function, GlobalValue, Value, ValueIter, ValueRef};
use types::{FunctionTy, Ty};
use util::chars;

#[derive(Clone)]
pub struct Module(pub LLVMModuleRef);
impl_dispose!(Module, core::LLVMDisposeModule);
impl_from_ref!(LLVMModuleRef, Module);

impl Module {
  pub fn new(ctx: LLVMContextRef, name: &str) -> Module {
    let c_name = chars::from_str(name);
    Module(unsafe { core::LLVMModuleCreateWithNameInContext(c_name, ctx) })
  }

  pub fn new_from_bc(ctx: LLVMContextRef, path: &str) -> Result<Module, String> {
    unsafe {
      let mut m: LLVMModuleRef = mem::uninitialized();
      let mut err: *mut c_char = mem::uninitialized();
      let buf = try!(MemoryBuffer::from_file(path));

      let ret = LLVMParseBitcodeInContext(ctx, buf.as_ptr(), &mut m, &mut err);
      llvm_ret!(ret, Module(m), err)
    }
  }

  /// Returns the target data of the base module represented as a string
  pub fn target(&self) -> &str {
    unsafe {
      let target = core::LLVMGetTarget(self.0);
      chars::to_str(target)
    }
  }

  /// Get the data layout of the base module
  pub fn data_layout(&self) -> &str {
    unsafe {
      let layout = core::LLVMGetDataLayout(self.0);
      chars::to_str(layout as *mut c_char)
    }
  }

  /// Link a module into this module, returning an error string if an error occurs.
  ///
  /// This *does not* destroy the source module.
  pub fn link(&self, other: Module) -> Result<(), String> {
    unsafe {
      let mut error = mem::uninitialized();
      let ret = linker::LLVMLinkModules(self.0,
                                        other.0,
                                        linker::LLVMLinkerMode::LLVMLinkerPreserveSource,
                                        &mut error);
      llvm_ret!(ret, (), error)
    }
  }

  /// Link a module into this module, returning an error string if an error occurs.
  ///
  /// This *does* destroy the source module.
  pub fn link_destroy(&self, other: Module) -> Result<(), String> {
    unsafe {
      let mut error = mem::uninitialized();
      let ret = linker::LLVMLinkModules(self.0,
                                        other.0,
                                        linker::LLVMLinkerMode::LLVMLinkerDestroySource,
                                        &mut error);
      llvm_ret!(ret, (), error)
    }
  }

  /// Optimize this module with the given optimization level and size level.
  ///
  /// This runs passes depending on the levels given.
  pub fn optimize(&self, opt_level: usize, size_level: usize) {
    unsafe {
      let builder = pass::LLVMPassManagerBuilderCreate();
      pass::LLVMPassManagerBuilderSetOptLevel(builder, opt_level as c_uint);
      pass::LLVMPassManagerBuilderSetSizeLevel(builder, size_level as c_uint);
      let pass_manager = core::LLVMCreatePassManager();
      pass::LLVMPassManagerBuilderPopulateModulePassManager(builder, pass_manager);
      pass::LLVMPassManagerBuilderDispose(builder);
      core::LLVMRunPassManager(pass_manager, self.0);
    }
  }

  /// Verify that the module is safe to run, returning a string detailing the error
  /// when an error occurs.
  pub fn verify(&self) -> Result<(), String> {
    Verifier::verify_module(self.0)
  }

  /// Dump the module to stderr (for debugging).
  pub fn dump(&self) {
    unsafe {
      core::LLVMDumpModule(self.0);
    }
  }

  /// Returns the type with the name given, or `None`` if no type with that name exists.
  pub fn get_ty(&self, name: &str) -> Option<Ty> {
    let c_name = chars::from_str(name);
    unsafe {
      let ty = core::LLVMGetTypeByName(self.0, c_name);
      ::util::ret_nullable_ptr(ty)
    }
  }

  /// Add an external global to the module with the given type and name.
  pub fn add_global(&self, name: &str, ty: &Ty) -> GlobalValue {
    let c_name = chars::from_str(name);
    GlobalValue(unsafe { core::LLVMAddGlobal(self.0, ty.0, c_name) })
  }

  /// Add a global in the given address space to the module with the given type and name.
  pub fn add_global_in_addr_space(&self, name: &str, ty: &Ty, sp: AddressSpace) -> GlobalValue {
    let c_name = chars::from_str(name);
    GlobalValue(unsafe {
      core::LLVMAddGlobalInAddressSpace(self.0, ty.0, c_name, sp as c_uint)
    })
  }

  /// Add a constant global to the module with the given type, name and value.
  pub fn add_global_constant(&self, name: &str, val: &Value) -> GlobalValue {
    let c_name = chars::from_str(name);
    GlobalValue(unsafe {
      let global = core::LLVMAddGlobal(self.0, val.ty().0, c_name);
      core::LLVMSetInitializer(global, val.0);
      global
    })
  }

  /// Get the global with the name given, or `None` if no global with that name exists.
  pub fn get_global(&self, name: &str) -> Option<GlobalValue> {
    let c_name = chars::from_str(name);
    unsafe {
      let ptr = core::LLVMGetNamedGlobal(self.0, c_name);
      ::util::ret_nullable_ptr(ptr)
    }
  }

  /// Get an iterator of global values
  pub fn global_values(&self) -> ValueIter<GlobalValue> {
    ValueIter::new(unsafe { core::LLVMGetFirstGlobal(self.0) },
                   core::LLVMGetNextGlobal)
  }

  /// Add a function to the module with the name given.
  pub fn add_func(&self, name: &str, sig: &FunctionTy) -> Function {
    let c_name = chars::from_str(name);
    Function(unsafe { core::LLVMAddFunction(self.0, c_name, sig.0) })
  }

  /// Returns the function with the name given, or `None` if no function with that name exists.
  pub fn get_func(&self, name: &str) -> Option<Function> {
    let c_name = chars::from_str(name);
    unsafe {
      let ty = core::LLVMGetNamedFunction(self.0, c_name);
      ::util::ret_nullable_ptr(ty)
    }
  }
}