
#![feature(type_info)]

use std::mem::type_info::{Type, TypeKind, Generic};
pub use alias_macro::*;

pub const fn type_has_pointer(ty: Type) -> bool {
    match ty.kind {
        TypeKind::Pointer(_) => true,

        TypeKind::Reference(r) => type_has_pointer(r.pointee.info()),
        
        TypeKind::Slice(s) => type_has_pointer(s.element_ty.info()),

        TypeKind::Array(a) => type_has_pointer(a.element_ty.info()),

        TypeKind::Struct(s) => {
            let fields = s.fields;
            let mut i = 0;
            while i < fields.len() {
                if type_has_pointer(fields[i].ty.info()) {
                    return true;
                }
                i += 1;
            }
            false
        }

        TypeKind::Tuple(t) => {
            let fields = t.fields;
            let mut i = 0;
            while i < fields.len() {
                if type_has_pointer(fields[i].ty.info()) {
                    return true;
                }
                i += 1;
            }
            false
        }

        TypeKind::Union(u) => {
            let fields = u.fields;
            let mut i = 0;
            while i < fields.len() {
                if type_has_pointer(fields[i].ty.info()) {
                    return true;
                }
                i += 1;
            }
            false
        }

        TypeKind::Enum(e) => {
            let generics = e.generics;
            let mut i = 0;
            while i < generics.len() {
                if let Generic::Type(g_ty) = &generics[i] {
                    if type_has_pointer(g_ty.ty.info()) {
                        return true;
                    }
                }
                i += 1;
            }
            false
        }

        _ => false,
    }
}

pub struct ImmutGuard<'a, T: ?Sized + 'static> {
    ptr: std::ptr::NonNull<T>,

    // SAFETY:
    // - Semantically owns &'a T
    // - Shared borrow — boleh alias
    // - Boleh di-clone karena &T bisa alias
    _marker: std::marker::PhantomData<&'a T>,
}

impl<'a, T: ?Sized> ImmutGuard<'a, T> {
#[inline(always)]
    pub fn immutable_reference(&self) -> &T {
        // SAFETY:
        // The original `&mut T` guarantees:
        // - pointer validity
        // - proper alignment
        // - initialized memory
        //
        // Returning `&T` from `&self` is safe because:
        // - immutable references may alias other immutable references
        // - Rust reference rules prevent obtaining `&mut self` simultaneously with this reference in safe code
        unsafe { self.ptr.as_ref() }
    }
    
    #[inline(always)]
    pub fn with_immutable_reference<R>(&self, f: impl for<'reff> FnOnce(&'reff T) -> R) -> R
    {
        // SAFETY:
        // Same reasoning as `immutable_reference`.
        //
        // The reference is scoped to the closure call,
        // preventing it from escaping accidentally.
        const { assert_no_pointer::<R>() };
        unsafe { f(self.ptr.as_ref()) }
    }
    
    #[inline(always)]
    pub fn with_immutable_pointer<R>(&self, f: impl FnOnce(*const T) -> R) -> R 
    {
        // SAFETY:
        // - Rust reference rules prevent obtaining `&mut self` simultaneously with this reference in safe code
        //
        // In particular:
        // - The immutable raw pointer is scoped to the closure execution
        // which makes able to create `&mut` without invalidating the pointers
        // - It prevents calling immutable raw pointer while `&mut` is still active because it violates the aliasing rules
        const { assert_no_pointer::<R>() };
        f(self.ptr.as_ptr())
    }
    
    pub fn clone_guard(&self) -> ImmutGuard<'_, T> {
        ImmutGuard {
            ptr: self.ptr,
            _marker: std::marker::PhantomData,
        }
    }

    
    #[inline(always)]
    pub fn close(self) {}



}

pub struct AliasingGuard<'a, T: ?Sized + 'static> {
    ptr: std::ptr::NonNull<T>,

    // SAFETY:
    // This models exclusive mutable ownership over `T` for lifetime `'a`.
    //
    // The guard conceptually behaves like it owns an `&'a mut T`,
    // which prevents aliasing mutable borrows through Rust's borrow checker.
    //
    // `PhantomData<&'a mut T>` is important because:
    // - it enforces invariance over `T`
    // - it tells the compiler this type semantically contains `&mut T`
    // - it enables borrow checking rules for aliasing/exclusivity
    // - it prevents multiple mutable guards existing simultaneously in safe code
    _marker: std::marker::PhantomData<&'a mut T>,
}

impl<'a, T: ?Sized> AliasingGuard<'a, T> {
    #[inline(always)]
    pub fn from_ref(value: &'a mut T) -> Self {
        Self {
            // SAFETY:
            // `NonNull::from` is safe because `&mut T` is guaranteed:
            // - non-null
            // - properly aligned
            // - valid for reads/writes for `'a`
            ptr: std::ptr::NonNull::from(value),

            _marker: std::marker::PhantomData,
        }
    }
    
    #[inline(always)]
    pub fn immutable_guard(&self) -> ImmutGuard<'_, T> {
        ImmutGuard {
            ptr: self.ptr,
            _marker: std::marker::PhantomData,
        }
    }

    #[inline(always)]
    pub fn immutable_reference(&self) -> &T {
        // SAFETY:
        // The original `&mut T` guarantees:
        // - pointer validity
        // - proper alignment
        // - initialized memory
        //
        // Returning `&T` from `&self` is safe because:
        // - immutable references may alias other immutable references
        // - Rust reference rules prevent obtaining `&mut self` simultaneously with this reference in safe code
        unsafe { self.ptr.as_ref() }
    }

    #[inline(always)]
    pub fn mutable_reference(&mut self) -> &mut T {
        // SAFETY:
        // `&mut self` guarantees exclusive access to the guard.
        //
        // Because the guard semantically owns an exclusive `&mut T`,
        // this ensures no competing mutable references can exist
        // through this API in safe Rust.
        //
        // WARNING:
        // Raw pointers previously extracted from this guard may still
        // exist and can violate aliasing rules if used incorrectly.
        // Safe Rust callers cannot trigger UB here, but unsafe callers can.
        unsafe { self.ptr.as_mut() }
    }

    #[inline(always)]
    pub fn with_immutable_reference<R>(&self, f: impl for<'reff> FnOnce(&'reff T) -> R) -> R
    {
        // SAFETY:
        // Same reasoning as `immutable_reference`.
        //
        // The reference is scoped to the closure call,
        // preventing it from escaping accidentally.
        const { assert_no_pointer::<R>() };
        unsafe { f(self.ptr.as_ref()) }
    }

    #[inline(always)]
    pub fn with_mutable_reference<R>(&mut self, f: impl for<'reff> FnOnce(&'reff mut T) -> R) -> R 
    {
        // SAFETY:
        // Same reasoning as `mutable_reference`.
        //
        // The mutable reference is scoped to the closure execution,
        // which helps reduce accidental misuse duration.
        const { assert_no_pointer::<R>() };
        unsafe { f(self.ptr.as_mut()) }
    }

    #[inline(always)]
    pub fn with_immutable_pointer<R>(&self, f: impl FnOnce(*const T) -> R) -> R 
    {
        // SAFETY:
        // - Rust reference rules prevent obtaining `&mut self` simultaneously with this reference in safe code
        //
        // In particular:
        // - The immutable raw pointer is scoped to the closure execution
        // which makes able to create `&mut` without invalidating the pointers
        // - It prevents calling immutable raw pointer while `&mut` is still active because it violates the aliasing rules
        const { assert_no_pointer::<R>() };
        f(self.ptr.as_ptr())
    }

    #[inline(always)]
    pub fn with_mutable_pointer<R>(&mut self, f: impl FnOnce(*mut T) -> R) -> R 
    {
        // SAFETY:
        // - Rust reference rules prevent obtaining `&mut self` simultaneously with this reference in safe code
        //
        // In particular:
        // - The mutable raw pointer is scoped to the closure execution
        // which makes able to create `&` or `&mut` without invalidating the pointers
        // - It prevents calling mutable raw pointer while `&` or `&mut` is still active because it violates the aliasing rules
        const { assert_no_pointer::<R>() };
        f(self.ptr.as_ptr())
    }

    #[inline(always)]
    pub unsafe fn as_ptr(&mut self) -> *mut T {
        // SAFETY:
        // This exists to make if closure based pointer is not enough, then this unsafe method can be used
        // Returning raw pointers is safe by itself.
        //
        // However, once the pointer escapes, this type can no longer
        // enforce aliasing guarantees.
        //
        // The caller must ensure:
        // - no invalid reference/raw-pointer combinations are used
        // - no aliasing UB occurs
        // - do not write to the pointer while `&` or `&mut` to same memory is still active
        // - do not read the pointer while `&mut` to same memory is still active
        // - be aware that `&mut` creation that points to same address of this pointer will invalidate this pointer
        // - pointer is not used after underlying value becomes invalid
        self.ptr.as_ptr()
    }
    

    #[inline(always)]
    pub fn close(self) {
        // SAFETY:
        // Consuming `self` ends the guard lifetime early.
        //
        // This can be useful to release the conceptual mutable borrow
        // before the surrounding scope ends.
    }
}


const fn assert_no_pointer<T: ?Sized>() {
    if type_has_pointer(Type::of::<T>()) {
        panic!("return type can't contain raw pointer");
    }
}

const fn assert_no_pointer2<T: ?Sized>() {
    if type_has_pointer(Type::of::<T>()) {
        panic!("can not use raw pointer inside this closure, other than raw pointer that is returned by the closure");
    }
}

pub fn assert_no_pointer_val<T: ?Sized>(_: &T) {
    const {
        assert_no_pointer2::<T>();
    }
}

#[macro_export]
macro_rules! guard {
    ($guard:ident & $var:ident $body:block) => {
        $crate::guard_block!({
            $guard.with_immutable_reference(|$var| $body)
        });
    };
    
    ($guard:ident &mut $var:ident $body:block) => {
        $crate::guard_block!({
            $guard.with_mutable_reference(|$var| $body)
        });
    };
    
    ($guard:ident *const $var:ident $body:expr) => {
        $crate::guard_block_no_reference!({
            $guard.with_immutable_pointer(|$var| $body)
        });
    };
    
    ($guard:ident *mut $var:ident $body:expr) => {
        $crate::guard_block_no_reference!({
            $guard.with_mutable_pointer(|$var| $body)
        });
    };
    
    ($guard:ident & $var:ident read only $body:expr) => {
        $crate::guard_block_no_write!({
            $guard.with_immutable_reference(|$var| $body)
        });
    };
    
    ($guard:ident *const $var:ident read only $body:expr) => {
        $crate::guard_block_no_write!({
            $guard.with_immutable_pointer(|$var| $body)
        });
    };
}
