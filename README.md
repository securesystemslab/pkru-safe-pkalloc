# pkalloc

Provides a rust bindings to a custom version of jemalloc that can provide pages protected with intel mpk keys

This library was originally part of pkru-safe, but was split from it to make integration with existing libraries more straightforward.

## Build Requirements
- cmake
- Intel MPK
- C++ compiler (C++-14 or higher)
- autotools (configure, autoconf, etc.)
- nightly rust compiler


## Build Instructions

`cargo build`
