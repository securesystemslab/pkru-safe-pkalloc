// MIT License
// lib.rs - pkalloc
//
// Copyright 2018 Paul Kirth
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE

#![never_gate]
#![no_std]
#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unused_attributes)]
#![deny(warnings)]
//#![feature(alloc)]
#![feature(allocator_api)]
//#![feature(alloc_system)]
#![feature(libc)]
#![crate_name = "pkalloc"]
#![crate_type = "rlib"]

extern crate alloc;
//extern crate alloc_system;
extern crate libc;

//usestd::alloc::System;

pub use contents::*;
mod contents {
    use core::ptr;

    use alloc::alloc::{Alloc, AllocErr, Layout};
    use libc::{c_char, c_int, c_uint, c_ulong, c_void, size_t};

    // Note that the symbols here are prefixed by default on macOS and Windows (we
    // don't explicitly request it), and on Android and DragonFly we explicitly
    // request it as unprefixing cause segfaults (mismatches in allocators).
    extern "C" {
        fn je_malloc(size: size_t) -> *mut c_void;
        fn je_realloc(ptr: *mut c_void, size: size_t) -> *mut c_void;
        fn je_free(ptr: *mut c_void);
        fn je_mallocx(size: size_t, flags: c_int) -> *mut c_void;
        fn je_calloc(size: size_t, flags: c_int) -> *mut c_void;
        fn je_rallocx(ptr: *mut c_void, size: size_t, flags: c_int) -> *mut c_void;
        fn je_xallocx(ptr: *mut c_void, size: size_t, extra: size_t, flags: c_int) -> size_t;
        fn je_sdallocx(ptr: *mut c_void, size: size_t, flags: c_int);
        fn je_nallocx(size: size_t, flags: c_int) -> size_t;
        fn je_malloc_usable_size(ptr: *const c_void) -> size_t;
        fn vma_pkey() -> c_int;
        fn is_safe_address(addr: *mut c_void) -> bool;

        // mallctl
        #[link_name = "je_mallctl"]
        pub fn mallctl(
            name: *const c_char,
            oldp: *mut c_void,
            oldpenp: *mut size_t,
            newp: *mut c_void,
            newlen: size_t,
        ) -> c_int;

        #[link_name = "je_mallctlnametomib"]
        pub fn mallctlnametomib(
            name: *const c_char,
            mibp: *mut size_t,
            miblenp: *mut size_t,
        ) -> c_int;

        #[link_name = "je_mallctlbymib"]
        pub fn mallctlbymib(
            mib: *const size_t,
            miblen: size_t,
            oldp: *mut c_void,
            oldpenp: *mut size_t,
            newp: *mut c_void,
            newlen: size_t,
        ) -> c_int;

        // stats
        #[link_name = "je_malloc_stats_print"]
        pub fn malloc_stats_print(
            write_cb: extern "C" fn(*mut c_void, *const c_char),
            cbopaque: *mut c_void,
            opts: *const c_char,
        );

    }

    const MALLOCX_ZERO: c_int = 0x40;

    // The minimum alignment guaranteed by the architecture. This value is used to
    // add fast paths for low alignment values.
    #[cfg(all(any(target_arch = "arm", target_arch = "mips", target_arch = "powerpc")))]
    const MIN_ALIGN: usize = 8;
    #[cfg(
        all(
            any(
                target_arch = "x86",
                target_arch = "x86_64",
                target_arch = "aarch64",
                target_arch = "powerpc64",
                target_arch = "mips64",
                target_arch = "s390x",
                target_arch = "sparc64"
            )
        )
    )]
    const MIN_ALIGN: usize = 16;

    // MALLOCX_ALIGN(a) macro
    #[inline]
    fn je_mallocx_align(a: usize) -> c_int {
        a.trailing_zeros() as c_int
    }

    #[inline]
    fn je_align_to_flags(align: usize, size: usize) -> c_int {
        if align <= MIN_ALIGN && align <= size {
            0
        } else {
            je_mallocx_align(align)
        }
    }

    // for symbol names src/librustc/middle/allocator.rs
    // for signatures src/librustc_allocator/lib.rs

    // linkage directives are provided as part of the current compiler allocator
    // ABI

    #[inline]
    pub unsafe extern "C" fn pk_alloc(size: usize, align: usize, _err: *mut u8) -> *mut u8 {
        let flags = je_align_to_flags(align, size);
        let ptr = je_mallocx(size as size_t, flags) as *mut u8;
        ptr
    }

    #[inline]
    pub unsafe extern "C" fn pk_dealloc(ptr: *mut u8, size: usize, align: usize) {
        let flags = je_align_to_flags(align, size);
        je_sdallocx(ptr as *mut c_void, size, flags);
    }

    #[inline]
    pub unsafe extern "C" fn pk_usable_size(layout: *const u8, min: *mut usize, max: *mut usize) {
        let layout = &*(layout as *const Layout);
        let flags = je_align_to_flags(layout.align(), layout.size());
        let size = je_nallocx(layout.size(), flags) as usize;
        *min = layout.size();
        if size > 0 {
            *max = size;
        } else {
            *max = layout.size();
        }
    }

    #[inline]
    pub unsafe extern "C" fn pk_realloc(
        ptr: *mut u8,
        layout: Layout,
        new_size: usize,
    ) -> *mut u8 {
        let flags = je_align_to_flags(layout.align(), new_size);
        je_rallocx(ptr as *mut c_void, new_size, flags) as *mut u8
    }

    #[inline]
    pub unsafe extern "C" fn pk_alloc_zeroed(size: usize, align: usize) -> *mut u8 {
        if align <= MIN_ALIGN && align <= size {
            je_calloc(size as size_t, 1) as *mut u8
        } else {
            let flags = je_align_to_flags(align, size) | MALLOCX_ZERO;
            je_mallocx(size as size_t, flags) as *mut u8
        }
    }

    #[inline]
    pub unsafe extern "C" fn pk_alloc_excess(
        size: usize,
        align: usize,
        excess: *mut usize,
        err: *mut u8,
    ) -> *mut u8 {
        let p = pk_alloc(size, align, err);
        if !p.is_null() {
            let flags = je_align_to_flags(align, size);
            *excess = je_nallocx(size, flags) as usize;
        }
        return p;
    }

    #[inline]
    pub unsafe extern "C" fn pk_realloc_excess(
        ptr: *mut u8,
        layout: Layout,
        new_size: usize,
        excess: *mut usize,
    ) -> *mut u8 {
        let new_align = layout.align();
        let p = pk_realloc(ptr, layout, new_size);
        if !p.is_null() {
            let flags = je_align_to_flags(new_align, new_size);
            *excess = je_nallocx(new_size, flags) as usize;
        }
        p
    }

    #[inline]
    pub unsafe extern "C" fn pk_grow_in_place(
        ptr: *mut u8,
        old_size: usize,
        old_align: usize,
        new_size: usize,
        new_align: usize,
    ) -> u8 {
        pk_shrink_in_place(ptr, old_size, old_align, new_size, new_align)
    }

    #[inline]
    pub unsafe extern "C" fn pk_shrink_in_place(
        ptr: *mut u8,
        _old_size: usize,
        old_align: usize,
        new_size: usize,
        new_align: usize,
    ) -> u8 {
        if old_align == new_align {
            let flags = je_align_to_flags(new_align, new_size);
            (je_xallocx(ptr as *mut c_void, new_size, 0, flags) == new_size) as u8
        } else {
            0
        }
    }

    #[inline]
    pub unsafe extern "C" fn pk_vma_pkey() -> i32 {
        vma_pkey() as i32
    }

    #[inline]
    pub unsafe extern "C" fn pk_is_safe_addr(addr: *mut u8) -> bool {
        is_safe_address(addr as *mut c_void)
    }

    #[inline]
    pub unsafe extern "C" fn pk_malloc_usable_size(ptr: *const c_void) -> usize {
        je_malloc_usable_size(ptr)
    }

    pub mod libc_compat {
        use super::*;

        #[inline]
        pub unsafe extern "C" fn malloc(size: size_t) -> *mut c_void {
            je_malloc(size)
        }

        #[inline]
        pub unsafe extern "C" fn realloc(ptr: *mut c_void, size: size_t) -> *mut c_void {
            je_realloc(ptr, size)
        }

        #[inline]
        pub unsafe extern "C" fn free(ptr: *mut c_void) {
            je_free(ptr)
        }

    }
}
