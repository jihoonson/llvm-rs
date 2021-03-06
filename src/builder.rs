#![allow(dead_code)]

use std::mem;

use llvm_sys::{LLVMIntPredicate, LLVMOpcode, LLVMRealPredicate, core};
use llvm_sys::prelude::{LLVMBuilderRef, LLVMContextRef, LLVMValueRef};
use libc::{c_char, c_uint};

use super::LLVMRef;
use types::Ty;
use block::BasicBlock;
use value::{Function, PhiNode, Predicate, Value, ValueRef};

static NULL_NAME: [c_char; 1] = [0];

/// See http://llvm.org/docs/LangRef.html#conversion-operations
pub enum CastOp {
  /// The ‘trunc‘ instruction takes a value to trunc, and a type to trunc it to.
  /// Both types must be of integer types, or vectors of the same number of
  /// integers. The bit size of the value must be larger than the bit size of
  /// the destination type, ty2. Equal sized types are not allowed.
  Trunc,
  /// The ‘zext‘ instruction takes a value to cast, and a type to cast it to.
  /// Both types must be of integer types, or vectors of the same number of
  /// integers. The bit size of the value must be smaller than the bit size
  /// of the destination type, ty2.
  ZExt,
  /// The ‘sext‘ instruction takes a value to cast, and a type to cast it to.
  /// Both types must be of integer types, or vectors of the same number of
  /// integers. The bit size of the value must be smaller than the bit size
  /// of the destination type, ty2.
  SExt,
  /// The ‘fptrunc‘ instruction takes a floating point value to cast and
  /// a floating point type to cast it to. The size of value must be larger
  /// than the size of ty2. This implies that fptrunc cannot be used to make
  /// a no-op cast.
  FPTrunc,
  /// The ‘fpext‘ instruction extends the value from a smaller floating point
  /// type to a larger floating point type. The fpext cannot be used to make
  /// a no-op cast because it always changes bits. Use bitcast to make a no-op
  /// cast for a floating point cast.
  FPExt,
  /// The ‘uitofp‘ instruction takes a value to cast, which must be a scalar
  /// or vector integer value, and a type to cast it to ty2, which must be
  /// an floating point type. If ty is a vector integer type, ty2 must be
  /// a vector floating point type with the same number of elements as ty.
  UIToFP,
  /// The ‘sitofp‘ instruction takes a value to cast, which must be a scalar
  /// or vector integer value, and a type to cast it to ty2, which must be
  /// an floating point type. If ty is a vector integer type, ty2 must be
  /// a vector floating point type with the same number of elements as ty.
  SIToFP,
  /// The ‘fptoui‘ instruction takes a value to cast, which must be a scalar
  /// or vector floating point value, and a type to cast it to ty2, which
  /// must be an integer type. If ty is a vector floating point type, ty2
  /// must be a vector integer type with the same number of elements as ty.
  FPToUI,
  /// The ‘fptosi‘ instruction takes a value to cast, which must be a scalar
  /// or vector floating point value, and a type to cast it to ty2, which must
  /// be an integer type. If ty is a vector floating point type, ty2 must be a
  /// vector integer type with the same number of elements as ty.
  FPToSI,
  /// The ‘ptrtoint‘ instruction takes a value to cast, which must be a value
  /// of type pointer or a vector of pointers, and a type to cast it to ty2,
  /// which must be an integer or a vector of integers type.
  PtrToInt,
  /// The ‘inttoptr‘ instruction takes an integer value to cast, and a type
  /// to cast it to, which must be a pointer type.
  IntToPtr,
  /// The ‘bitcast‘ instruction takes a value to cast, which must be a
  /// non-aggregate first class value, and a type to cast it to, which must
  /// also be a non-aggregate first class type. The bit sizes of value and
  /// the destination type, ty2, must be identical. If the source type is a
  /// pointer, the destination type must also be a pointer of the same size.
  /// This instruction supports bitwise conversion of vectors to integers and
  /// to vectors of other types (as long as they have the same size).
  BitCast,
}

pub struct Builder(pub LLVMBuilderRef);
impl_dispose!(Builder, core::LLVMDisposeBuilder);

macro_rules! unary_instr (
  ($name:ident, $func:ident) => (
    pub fn $name(&self, value: &Value) -> Value {
      Value(unsafe {
        core::$func(self.0, value.0, NULL_NAME.as_ptr() as *const c_char)
      })
    }
  );
);

macro_rules! bin_instr (
  ($name:ident, $func:ident) => (
    pub fn $name(&self, lhs: &Value, rhs: &Value) -> Value
    {
      Value(unsafe {
        core::$func(self.0, lhs.0, rhs.0, NULL_NAME.as_ptr())
      })
    }
  );
  ($name:ident, $ifunc:ident, $ffunc:ident) => (
    pub fn $name(&self, lhs: &Value, rhs: &Value) -> Value {
      let lhs_ty = lhs.ty();
      let rhs_ty = rhs.ty();
      debug_assert_eq!(lhs_ty, rhs_ty);

      let instr_fn = if lhs_ty.is_integer() {
        core::$ifunc
      } else {
        core::$ffunc
      };

      Value(unsafe {
        instr_fn(self.0, lhs.0, rhs.0, NULL_NAME.as_ptr())
      })
    }
  );
);

impl Builder {
  pub fn new(ctx: LLVMContextRef) -> Builder {
    Builder(unsafe { core::LLVMCreateBuilderInContext(ctx) })
  }

  pub fn get_insert_block(&self) -> BasicBlock {
    BasicBlock(unsafe { core::LLVMGetInsertBlock(self.0) })
  }

  /// Position the builder at `instr` within `block`.
  pub fn position_at(&self, block: &BasicBlock, instr: &Value) {
    unsafe { core::LLVMPositionBuilder(self.0, block.0, instr.0) }
  }

  /// Position the builder at the end of `block`.
  pub fn position_at_end(&self, block: &BasicBlock) {
    unsafe { core::LLVMPositionBuilderAtEnd(self.0, block.0) }
  }

  /// Build an instruction that returns from the function with void.
  pub fn create_ret_void(&self) -> Value {
    Value(unsafe { core::LLVMBuildRetVoid(self.0) })
  }

  /// Build an instruction that returns from the function with `value`.
  pub fn create_ret(&self, value: &Value) -> Value {
    Value(unsafe { core::LLVMBuildRet(self.0, value.0) })
  }

  /// Build an instruction that allocates an array with the element type `elem` and
  /// the size `size`.
  ///
  /// The size of this array will be the size of `elem` times `size`.
  pub fn build_array_alloca(&self, elem: &Ty, size: &Value) -> Value {
    Value(unsafe {
      core::LLVMBuildArrayAlloca(self.0, elem.0, size.0, NULL_NAME.as_ptr() as *const c_char)
    })
  }

  /// Build an instruction that allocates a pointer to fit the size of `ty` then returns this
  /// pointer.
  ///
  /// Make sure to call `build_free` with the pointer value when you're done with it, or you're
  /// gonna have a bad time.
  pub fn create_alloca(&self, ty: &Ty) -> Value {
    Value(unsafe { core::LLVMBuildAlloca(self.0, ty.0, NULL_NAME.as_ptr() as *const c_char) })
  }

  /// Build an instruction that frees the `val`, which _MUST_ be a pointer that was returned
  /// from `build_alloca`.
  pub fn create_free(&self, val: &Value) -> Value {
    Value(unsafe { core::LLVMBuildFree(self.0, val.0) })
  }

  /// Build an instruction that store the value `val` in the pointer `ptr`.
  pub fn create_store(&self, val: &Value, ptr: &Value) -> Value {
    debug_assert!(ptr.ty().is_pointer(), "The target must be a pointer type");
    Value(unsafe { core::LLVMBuildStore(self.0, val.0, ptr.0) })
  }

  /// Build an instruction that branches to the block `dest`.
  pub fn create_br(&self, dest: &BasicBlock) -> Value {
    Value(unsafe { core::LLVMBuildBr(self.0, dest.0) })
  }

  /// Build an instruction that branches to `if_block` if `cond` evaluates to true, and
  /// `else_block` otherwise.
  pub fn create_cond_br(&self,
                        cond: &Value,
                        if_block: &BasicBlock,
                        else_block: &BasicBlock)
                        -> Value {
    Value(unsafe {
      core::LLVMBuildCondBr(self.0, cond.0, if_block.0, mem::transmute(else_block.0))
    })
  }

  /// Build an instruction that runs whichever block matches the value, or `default` if none of
  /// them matched it.
  pub fn create_switch(&self,
                       value: &Value,
                       default: &BasicBlock,
                       cases: &[(&Value, &BasicBlock)])
                       -> Value {
    Value(unsafe {
      let switch = core::LLVMBuildSwitch(self.0, value.0, default.0, cases.len() as c_uint);
      for case in cases {
        core::LLVMAddCase(switch, (case.0).0, (case.1).0);
      }

      switch
    })
  }

  /// Build an instruction that calls the function `func` with the arguments `args`.
  ///
  /// This will return the return value of the function.
  fn create_call_internal<V: LLVMRef<LLVMValueRef>>(&self,
                                                    func: &Function,
                                                    args: &[&V],
                                                    tail_call: bool)
                                                    -> Value {
    let ref_array = to_llvmref_array!(args, LLVMValueRef);

    Value(unsafe {
      let call = core::LLVMBuildCall(self.0,
                                     func.0,
                                     ref_array.as_ptr() as *mut LLVMValueRef,
                                     args.len() as c_uint,
                                     NULL_NAME.as_ptr());
      core::LLVMSetTailCall(call,
                            if tail_call {
                              1
                            } else {
                              0
                            });
      call.into()
    })
  }

  /// Build an instruction that calls the function `func` with the arguments `args`.
  ///
  /// This will return the return value of the function.
  pub fn create_call(&self, func: &Function, args: &[&Value]) -> Value {
    self.create_call_internal(func, args, false)
  }

  /// Build an instruction that calls the function `func` with the arguments `args`.
  ///
  /// This will return the return value of the function.
  pub fn create_tail_call<V: LLVMRef<LLVMValueRef>>(&self, func: &Function, args: &[&V]) -> Value {
    self.create_call_internal(func, args, true)
  }

  /// Build an instruction that yields to `true_val` if `cond` is equal to `1`, and `false_val`
  /// otherwise.
  pub fn create_select(&self, cond: &Value, true_val: &Value, false_val: &Value) -> Value {
    Value(unsafe {
      core::LLVMBuildSelect(self.0, cond.0, true_val.0, false_val.0, NULL_NAME.as_ptr())
    })
  }

  pub fn create_cast(&self, op: CastOp, value: &Value, dest_ty: &Ty) -> Value {
    let llvm_op = match op {
      CastOp::Trunc => LLVMOpcode::LLVMTrunc,
      CastOp::ZExt => LLVMOpcode::LLVMZExt,
      CastOp::SExt => LLVMOpcode::LLVMSExt,
      CastOp::FPTrunc => LLVMOpcode::LLVMFPTrunc,
      CastOp::FPExt => LLVMOpcode::LLVMFPExt,
      CastOp::UIToFP => LLVMOpcode::LLVMUIToFP,
      CastOp::SIToFP => LLVMOpcode::LLVMSIToFP,
      CastOp::FPToUI => LLVMOpcode::LLVMFPToUI,
      CastOp::FPToSI => LLVMOpcode::LLVMFPToUI,
      CastOp::PtrToInt => LLVMOpcode::LLVMPtrToInt,
      CastOp::IntToPtr => LLVMOpcode::LLVMIntToPtr,
      CastOp::BitCast => LLVMOpcode::LLVMBitCast,
    };

    Value(unsafe { core::LLVMBuildCast(self.0, llvm_op, value.0, dest_ty.0, NULL_NAME.as_ptr()) })
  }

  /// Build an instruction that casts a value into a certain type.
  pub fn create_bit_cast(&self, value: &Value, dest: &Ty) -> Value {
    Value(unsafe { core::LLVMBuildBitCast(self.0, value.0, dest.0, NULL_NAME.as_ptr()) })
  }

  /// Build an instruction that inserts a value into an aggregate data value.
  pub fn create_insert_value(&self, agg: &Value, elem: &Value, index: usize) -> Value {
    Value(unsafe {
      core::LLVMBuildInsertValue(self.0, agg.0, elem.0, index as c_uint, NULL_NAME.as_ptr())
    })
  }

  /// Build an instruction that extracts a value from an aggregate type.
  pub fn create_extract_value(&self, agg: &Value, index: usize) -> Value {
    Value(unsafe {
      core::LLVMBuildExtractValue(self.0, agg.0, index as c_uint, NULL_NAME.as_ptr())
    })
  }

  unary_instr!{create_load, LLVMBuildLoad}
  unary_instr!{create_neg, LLVMBuildNeg}
  unary_instr!{create_not, LLVMBuildNot}

  bin_instr!{create_add, LLVMBuildAdd, LLVMBuildFAdd}
  bin_instr!{create_sub, LLVMBuildSub, LLVMBuildFSub}
  bin_instr!{create_mul, LLVMBuildMul, LLVMBuildFMul}
  bin_instr!{create_div, LLVMBuildSDiv, LLVMBuildFDiv}
  bin_instr!{create_rem, LLVMBuildSRem, LLVMBuildFRem}
  bin_instr!{create_shl, LLVMBuildShl}
  bin_instr!{create_ashr, LLVMBuildAShr}
  bin_instr!{create_and, LLVMBuildAnd}
  bin_instr!{create_or, LLVMBuildOr}
  bin_instr!{create_xor, LLVMBuildXor}


  /// Build an instruction to compare two values with the predicate given.
  pub fn create_cmp(&self, l: &Value, r: &Value, pred: Predicate) -> Value {
    self.create_cmp_internal(l, r, pred, true)
  }

  /// Build an instruction to compare two values with the predicate given.
  pub fn create_ucmp(&self, l: &Value, r: &Value, pred: Predicate) -> Value {
    self.create_cmp_internal(l, r, pred, false)
  }

  fn create_cmp_internal(&self, l: &Value, r: &Value, pred: Predicate, signed: bool) -> Value {
    let (lhs_ty, rhs_ty) = (l.ty(), r.ty());
    assert_eq!(lhs_ty, rhs_ty);

    if lhs_ty.is_integer() {
      let p = match (pred, signed) {
        (Predicate::Eq, _) => LLVMIntPredicate::LLVMIntEQ,
        (Predicate::Ne, _) => LLVMIntPredicate::LLVMIntNE,
        (Predicate::Lt, true) => LLVMIntPredicate::LLVMIntSLT,
        (Predicate::Lt, false) => LLVMIntPredicate::LLVMIntULT,
        (Predicate::Le, true) => LLVMIntPredicate::LLVMIntSLE,
        (Predicate::Le, false) => LLVMIntPredicate::LLVMIntULE,
        (Predicate::Gt, true) => LLVMIntPredicate::LLVMIntSGT,
        (Predicate::Gt, false) => LLVMIntPredicate::LLVMIntUGT,
        (Predicate::Ge, true) => LLVMIntPredicate::LLVMIntSGE,
        (Predicate::Ge, false) => LLVMIntPredicate::LLVMIntUGE,
      };

      Value(unsafe { core::LLVMBuildICmp(self.0, p, l.0, r.0, NULL_NAME.as_ptr()) })

    } else if lhs_ty.is_float() {
      let p = match pred {
        Predicate::Eq => LLVMRealPredicate::LLVMRealOEQ,
        Predicate::Ne => LLVMRealPredicate::LLVMRealONE,
        Predicate::Gt => LLVMRealPredicate::LLVMRealOGT,
        Predicate::Ge => LLVMRealPredicate::LLVMRealOGE,
        Predicate::Lt => LLVMRealPredicate::LLVMRealOLT,
        Predicate::Le => LLVMRealPredicate::LLVMRealOLE,
      };

      Value(unsafe { core::LLVMBuildFCmp(self.0, p, l.0, r.0, NULL_NAME.as_ptr()) })

    } else {
      panic!("expected numbers, got {:?}", lhs_ty)
    }
  }

  /// Build an instruction that computes the address of a subelement of an aggregate data
  /// structure.
  ///
  /// Basically type-safe pointer arithmetic.
  pub fn create_gep(&self, pointer: &Value, indices: &[&Value]) -> Value {
    let ref_array = to_llvmref_array!(indices, LLVMValueRef);

    Value(unsafe {
      core::LLVMBuildInBoundsGEP(self.0,
                                 pointer.0,
                                 ref_array.as_ptr() as *mut LLVMValueRef,
                                 indices.len() as c_uint,
                                 NULL_NAME.as_ptr())
    })
  }


  /// Build an instruction to select a value depending on the predecessor of the current block.
  pub fn create_phi(&self, ty: &Ty, name: &str) -> PhiNode {
    PhiNode(unsafe { core::LLVMBuildPhi(self.0, ty.0, ::util::chars::from_str(name)) })
  }
}

#[cfg(test)]
mod tests {
  use super::super::{FunctionTy, JitCompiler};
  use types::LLVMTy;
  use value::{Predicate, ToValue};

  #[test]
  pub fn test_cond_br() {
    let jit = JitCompiler::new("test1").ok().unwrap();
    let ctx = jit.context();

    let func_ty = FunctionTy::new(&u64::llvm_ty(ctx), &[&u64::llvm_ty(ctx)]);
    let func = jit.add_func("fib", &func_ty);
    let value = func.arg(0);

    let entry = func.append("entry");
    let then_bb = func.append("then_block");
    let else_bb = func.append("else_block");
    let merge_bb = func.append("merge_block");

    let builder = jit.builder();

    builder.position_at_end(&entry);
    let local = builder.create_alloca(&u64::llvm_ty(ctx));
    let cond = builder.create_cmp(&value.into(), &5u64.to_value(ctx), Predicate::Lt);
    builder.create_cond_br(&cond, &then_bb, &else_bb);

    builder.position_at_end(&then_bb);
    builder.create_store(&8u64.to_value(ctx), &local);
    builder.create_br(&merge_bb);

    builder.position_at_end(&else_bb);
    builder.create_store(&16u64.to_value(ctx), &local);
    builder.create_br(&merge_bb);

    builder.position_at_end(&merge_bb);
    let ret_val = builder.create_load(&local);
    builder.create_ret(&ret_val);

    jit.verify().unwrap();

    let fib: fn(u64) -> u64 = unsafe { ::std::mem::transmute(jit.get_func_ptr(&func).unwrap()) };

    for i in 0..10 {
      if i < 5 {
        assert_eq!(8, fib(i));
      } else {
        assert_eq!(16, fib(i));
      }
    }
  }
}
