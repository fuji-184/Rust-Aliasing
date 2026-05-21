# `aliasing_guard`

A Rust library for reducing accidental aliasing UB when mixing references and raw pointers.

This crate provides:

* `AliasingGuard<T>` for exclusive mutable access
* `ImmutGuard<T>` for shared immutable access
* compile-time pointer escape prevention
* procedural macro checks that reject dangerous raw-pointer patterns
* closure-scoped raw pointer access
* safer interoperability between references and raw pointers

This does **not** make unsafe code fully safe.

It is a defensive abstraction that prevents many common aliasing mistakes in safe-facing APIs.

---

# Motivation

Rust allows mixing:

* `&T`
* `&mut T`
* `*const T`
* `*mut T`

But aliasing rules become extremely easy to violate once raw pointers escape.

Example:

```rust
unsafe fn broken(ptr: *mut i32) {
    let a = &mut *ptr;
    let b = &mut *ptr;

    *a = 10;
    *b = 20;
}
```

This is undefined behavior.

The problem is not raw pointers themselves.

The problem is:

* pointer escaping
* uncontrolled lifetimes
* creating references while incompatible accesses still exist

This crate attempts to reduce those mistakes by forcing pointer/reference access into controlled scopes.

---

# Features

* Scoped mutable and immutable references
* Scoped raw pointer access
* Compile-time rejection of pointer-returning closures
* Compile-time rejection of common pointer escape patterns
* Macro-based diagnostics with custom error messages
* Prevents many accidental aliasing violations in safe-facing code
* Zero allocation
* Mostly zero-cost after optimization

---

# Requirements

Nightly Rust is required.

```rust
#![feature(type_info)]
```

---

# Installation

```toml
[dependencies]
aliasing_guard = { git = "https://github.com/fuji-184/Rust-Aliasing.git" }
```

---

# Basic Example

```rust
use aliasing_guard::*;

fn main() {
    let mut value = 10;

    let mut guard = AliasingGuard::from_ref(&mut value);

    let reff = guard.immutable_reference();
}
```

---

# Guard Types

## `AliasingGuard<T>`

Represents exclusive mutable ownership semantics.

Internally models:

```rust
PhantomData<&mut T>
```

This means:

* mutable aliasing is prevented by Rust borrow rules
* only one mutable guard can exist safely
* mutable access requires `&mut self`

---

## `ImmutGuard<T>`

```rust
let immutable_guard = guard.immutable_guard();
```

Represents shared immutable ownership semantics.

Internally models:

```rust
PhantomData<&T>
```

This means:

* immutable aliasing is allowed
* immutable guards may be cloned
* mutable access is impossible through this type

* all immutable guards (including the cloned one if it is still active) must be dropped first before able to use mutable guard, it will give compile time error

---

# APIs

## Create Guard

```rust
let mut guard = AliasingGuard::from_ref(&mut value);
```

---

## Immutable Reference

```rust
let reff = guard.immutable_reference();
```

---

## Mutable Reference

```rust
let mutable_reff = guard.mutable_reference();
```

---

## Scoped immutable Reference

```rust
guard!(guard &v {
    println!("{}", v);
});
```

---

## Scoped mutable Reference

```rust
guard!(guard &mut v {
    *v += 1;
});
```

---

## Scoped immutable Raw Pointer

```rust
guard!(guard *const ptr {
    unsafe {
        println!("{}", *ptr);
    }
});
```

---

## Scoped mutable Raw Pointer

```rust
guard!(guard *mut ptr {
    unsafe {
        *ptr += 1;
    }
});
```

---

## Scoped read only immutable Reference, can't be casted to mutable, can be casted to immutable Raw Pointer

```rust
guard!(guard &v read only {
    println!("{}", v);
});
```

---

## Scoped read only immutable Raw Pointer, can't be casted to mutable, can be casted to immutable Reference

```rust
guard!(guard *const ptr read only {
    unsafe {
        println!("{}", *ptr);
    }
});
```

---

# What This Crate Prevents

# 1. Returning Raw Pointers From Guard Closures

This is rejected:

```rust
let mut a = String::from("hello");
let mut guard = AliasingGuard::from_ref(&mut a);
let ptr = guard!(guard *mut a {
    a
});
```

Why?

Because escaping pointers can outlive the aliasing assumptions enforced by the guard.

The crate detects pointer-containing return types using type metadata.

---

# 2. Creating Raw Pointers Within Reference Guard

Rejected:

```rust
guard!(guard &mut value {
    let ptr = &raw mut *value;
});
```

Also rejects:

* `from_ref`
* `from_raw`
* `null_mut`
* `slice_from_raw_parts`
* `invalid_mut`
* `dangling`
* `.as_ptr()`
* `.as_mut_ptr()`
* `as *const T`
* `as *mut T`

---

# 3. Converting Raw Pointers Back Into References Within Reference Guard

Rejected:

```rust
guard!(guard *mut ptr {
    unsafe {
        let r = &mut *ptr;
    }
});
```

Also rejects:

* `.as_ref()`
* `.as_mut()`
* `as &T`
* `as &mut T`

This is important because creating references from raw pointers can invalidate aliasing assumptions.

---

# 4. Doing Mutable Reference or Mutable Pointer Operation While Any Reference To The Same Memory Is Active

Rejected:

```rust
let mut a = String::from("hello");
let b = &a;
let mut guard = AliasingGuard::from_ref(&mut a);
guard!(guard *mut a {
    let q = b;
});
```

---

# 5. Using Any Write Access &mut T or *mut T Within Read Only Guard

Rejected:

```rust
let mut a = String::from("hello");
let b = &raw mut a;
let mut guard = AliasingGuard::from_ref(&mut a);
guard!(guard &reff read only {
    let p = b;
    let q = &raw mut *reff ;
});
guard!(guard *const ptr read only {
    let p = b;
    let q = &raw mut *reff ;
});
```

---

# Closure Scoping

One of the main protections is closure scoping.

The pointer only exists during closure execution.

This reduces accidental long-lived pointer usage.

---

If the return type contains:

* raw pointers
* references
* arrays containing pointers
* structs containing pointers
* tuples containing pointers
* unions containing pointers

compilation fails.

---

# Supported Pointer Detection

The checker traverses:

* pointers
* references
* slices
* arrays
* structs
* tuples
* unions
* enum generics

Example rejected types:

```rust
*mut T
*const T
&T
&mut T
Option<*mut T>
Vec<&T>
(*mut T, i32)

struct Wrapper {
    ptr: *mut i32
}
```

---

# `close()`

The guard may be explicitly closed early.

```rust
guard.close();
```

This consumes the guard and ends the conceptual borrow early.

---

# Unsafe Escape Hatch

```rust
unsafe fn as_ptr(&mut self) -> *mut T
```

This bypasses most protections.

Once you call this:

* the pointer may escape
* aliasing guarantees are no longer enforceable
* the caller becomes fully responsible

Use only if closure-scoped access is insufficient.

---

# Limitations

This crate does NOT guarantee soundness.

It only reduces common mistakes.

---

# 1. Unsafe Code Can Still Break Everything

Example:

```rust
let ptr = unsafe { guard.as_ptr() };
```

After this, aliasing violations are entirely possible.

---

# 2. Procedural Macro Detection Is Pattern-Based

The macro detects many dangerous constructs, but not all possible unsafe tricks.

Some unsafe code can still bypass checks.

---

# 4. Enum Detection Is Limited

Current enum analysis only traverses generic parameters.

It does not inspect every variant field recursively.

---

# 5. Interior Mutability Is Not Prevented

Types like:

```rust
UnsafeCell<T>
Cell<T>
RefCell<T>
```

can still violate assumptions.

---

# 6. Pointer Aliasing Through FFI Is Not Prevented

External C/C++ code may still violate aliasing guarantees.

---

# 7. Raw Pointer Dereference Is Still Unsafe

This is still your responsibility:

```rust
unsafe {
    *ptr = 10;
}
```

The crate only attempts to reduce invalid pointer lifetime/aliasing patterns.

---

# 8. Type Metadata Requires Nightly

The recursive pointer detector depends on:

```rust
#![feature(type_info)]
```

This is unstable nightly-only functionality.

---

# 9. Macros Cannot Understand Full Program Semantics

The proc macro works syntactically.

It cannot fully prove:

* lifetime correctness
* aliasing validity
* concurrency correctness
* pointer provenance correctness

---

# Recommended Usage

Good fit:

* low-level containers
* intrusive collections
* FFI wrappers
* experimental aliasing APIs
* unsafe abstractions
* reducing accidental UB

Not recommended as:

* a security boundary
* a formal verifier
* a replacement for careful unsafe review

---

# Philosophy

This crate intentionally prefers:

* rejecting potential dangerous patterns
* scoped access
* preventing pointer escape
* explicit unsafe escape hatches

over unrestricted flexibility.

The goal is reducing accidental undefined behavior while still allowing controlled low-level programming.

