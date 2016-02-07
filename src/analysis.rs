//! Analysis Module

use std::mem;
use libc::{c_char, c_int};
use llvm_sys::analysis::{self, LLVMVerifierFailureAction};
use llvm_sys::prelude::{LLVMModuleRef, LLVMValueRef};

use value::Function;

// Extended APIs, offering more APIs than LLVM C API does.
extern "C" {
  pub fn LLVMVerifyFunction2(f: LLVMValueRef,
                             Action: LLVMVerifierFailureAction,
                             OutMessage: *mut *mut c_char)
                             -> c_int;
}

/// IR Verifier
pub struct Verifier;

impl Verifier {
  /// Verifies that a module is valid. When error, it will return an error
  /// message, useful for debugging.
  pub fn verify_module(module: LLVMModuleRef) -> Result<(), String> {
    unsafe {
      let mut error = mem::uninitialized();
      let action = analysis::LLVMVerifierFailureAction::LLVMReturnStatusAction;
      let res = analysis::LLVMVerifyModule(module, action, &mut error);

      llvm_ret!(res, (), error)
    }
  }

  /// Verifies that a single function is valid. When error, it will return an
  /// error message, useful for debugging.
  pub fn verify_func(func: &Function) -> Result<(), String> {
    unsafe {
      let mut error = ::std::mem::uninitialized();
      let res = LLVMVerifyFunction2(func.0,
                                    LLVMVerifierFailureAction::LLVMReturnStatusAction,
                                    &mut error);

      llvm_ret!(res, (), error)
    }
  }
}
