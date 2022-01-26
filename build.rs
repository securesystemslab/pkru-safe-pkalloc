// MIT License
// build.rs - pkalloc
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

extern crate cc;
extern crate cmake;
extern crate fs_extra;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn gnu_target(target: &str) -> String {
    match target {
        "i686-pc-windows-msvc" => "i686-pc-win32".to_string(),
        "x86_64-pc-windows-msvc" => "x86_64-pc-win32".to_string(),
        "i686-pc-windows-gnu" => "i686-w64-mingw32".to_string(),
        "x86_64-pc-windows-gnu" => "x86_64-w64-mingw32".to_string(),
        s => s.to_string(),
    }
}

fn main() {
    let sm_path = cmake_safemap();
    build_task(sm_path);
}

fn cmake_safemap() -> PathBuf {
    use cmake::Config;

    let mut dst = Config::new("allocator").build();
    dst.push("lib");
    println!("cargo:rustc-link-search=native={}", dst.display());
    println!("cargo:rustc-link-lib=static=safemap");
    dst.push("libsafemap.a");
    return dst;
}

fn link_safemap(safemap: PathBuf) {
    println!("cargo:rustc-link-search=native={}", safemap.display());

    let stem = safemap.file_stem().unwrap().to_str().unwrap();
    let name = safemap.file_name().unwrap().to_str().unwrap();
    let kind = if name.ends_with(".a") {
        "static"
    } else {
        "dylib"
    };
    println!("cargo:rustc-link-lib={}={}", kind, &stem[3..]);
}

fn build_task(safemap_path: PathBuf) {
    let target = env::var("TARGET").expect("TARGET was not set");
    let host = env::var("HOST").expect("HOST was not set");
    let num_jobs = env::var("NUM_JOBS").expect("NUM_JOBS was not set");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR was not set"));
    println!("TARGET={}", target.clone());
    println!("HOST={}", host.clone());
    println!("NUM_JOBS={}", num_jobs.clone());
    println!("OUT_DIR={:?}", out_dir);
    let build_dir = out_dir.join("build");
    println!("BUILD_DIR={:?}", build_dir);
    let src_dir = env::current_dir().expect("failed to get current directory");
    println!("SRC_DIR={:?}", src_dir);

    let unsupported_targets = [
        "rumprun",
        "bitrig",
        "openbsd",
        "msvc",
        "emscripten",
        "fuchsia",
        "redox",
        "wasm32",
    ];
    for i in &unsupported_targets {
        if target.contains(i) {
            panic!("jemalloc does not support target: {}", target);
        }
    }

    if let Some(jemalloc) = env::var_os("JEMALLOC_OVERRIDE") {
        println!("jemalloc override set");
        let jemalloc = PathBuf::from(jemalloc);
        println!(
            "cargo:rustc-link-search=native={}",
            jemalloc.parent().unwrap().display()
        );
        let stem = jemalloc.file_stem().unwrap().to_str().unwrap();
        let name = jemalloc.file_name().unwrap().to_str().unwrap();
        let kind = if name.ends_with(".a") {
            "static"
        } else {
            "dylib"
        };
        println!("cargo:rustc-link-lib={}={}", kind, &stem[3..]);
        return;
    }

    fs::create_dir_all(&build_dir).unwrap();
    let inc_dir = out_dir.join("include");
    // Disable -Wextra warnings - jemalloc doesn't compile free of warnings with
    // it enabled: https://github.com/jemalloc/jemalloc/issues/1196
    let compiler = cc::Build::new()
        .include(inc_dir)
        .extra_warnings(false)
        .get_compiler();
    let cflags = compiler
        .args()
        .iter()
        .map(|s| s.to_str().unwrap())
        .collect::<Vec<_>>()
        .join(" ");
    println!("CC={:?}", compiler.path());
    println!("CFLAGS={:?}", cflags);

    let jemalloc_src_dir = out_dir.join("jemalloc");
    println!("JEMALLOC_SRC_DIR={:?}", jemalloc_src_dir);

    if jemalloc_src_dir.exists() {
        fs::remove_dir_all(jemalloc_src_dir.clone()).unwrap();
    }

    // Copy jemalloc submodule to the OUT_DIR
    assert!(out_dir.exists(), "OUT_DIR does not exist");
    let mut copy_options = fs_extra::dir::CopyOptions::new();
    copy_options.overwrite = true;
    copy_options.copy_inside = true;
    fs_extra::dir::copy(
        Path::new("jemalloc"),
        jemalloc_src_dir.clone(),
        &copy_options,
    ).expect("failed to copy jemalloc source code to OUT_DIR");

    link_safemap(safemap_path.clone());

    //println!("\n\nsafemap_path = {:?}\n", safemap_path.parent().unwrap());
    let safemap_inc = cflags.clone() + " -L " + safemap_path.parent().unwrap().to_str().unwrap();
    println!("cflags = {}", safemap_inc);

    // Run configure:
    let configure = jemalloc_src_dir.join("configure");
    let mut cmd = Command::new("sh");
    cmd.arg(
        configure
            .to_str()
            .unwrap()
            .replace("C:\\", "/c/")
            .replace("\\", "/"),
    ).current_dir(&build_dir)
        .env("CC", compiler.path())
        .env("CFLAGS", cflags.clone())
        .env("LDFLAGS", safemap_inc)
        .env("CPPFLAGS", cflags.clone())
        .arg("--disable-cxx");

    if target == "sparc64-unknown-linux-gnu" {
        // jemalloc's configure doesn't detect this value
        // automatically for this target:
        cmd.arg("--with-lg-quantum=4");
        // See: https://github.com/jemalloc/jemalloc/issues/999
        cmd.arg("--disable-thp");
    }

    //cmd.arg("--enable-autogen");
    cmd.arg("--with-jemalloc-prefix=je_");

    if env::var_os("CARGO_FEATURE_DEBUG").is_some() {
        println!("CARGO_FEATURE_DEBUG set");
        cmd.arg("--enable-debug");
    }

    if env::var_os("CARGO_FEATURE_PROFILING").is_some() {
        println!("CARGO_FEATURE_PROFILING set set");
        cmd.arg("--enable-prof");
    }
    cmd.arg(format!("--host={}", gnu_target(&target)));
    cmd.arg(format!("--build={}", gnu_target(&host)));
    cmd.arg(format!("--prefix={}", out_dir.display()));

    run(&mut cmd);

    let make = if host.contains("bitrig")
        || host.contains("dragonfly")
        || host.contains("freebsd")
        || host.contains("netbsd")
        || host.contains("openbsd")
    {
        "gmake"
    } else {
        "make"
    };

    println!("\n\nbuild dir ={:?}\n", build_dir);
    // Make:
    run(Command::new(make)
        .current_dir(&build_dir)
        .arg("build_lib_static")
        .arg("-j")
        .arg(num_jobs.clone()));

    if env::var_os("JEMALLOC_SYS_RUN_TESTS").is_some() {
        println!("JEMALLOC_SYS_RUN_TESTS set: building and running jemalloc tests...");
        // Make tests:
        run(Command::new(make)
            .current_dir(&build_dir)
            .arg("-j")
            .arg(num_jobs.clone())
            .arg("tests"));

        // Run tests:
        run(Command::new(make).current_dir(&build_dir).arg("check"));
    }

    // Make install:
    run(Command::new(make)
        .current_dir(&build_dir)
        .arg("install_lib_static")
        .arg("install_include")
        .arg("-j")
        .arg(num_jobs.clone()));

    println!("cargo:rustc-link-lib=static=safemap");
    println!(
        "cargo:rustc-link-search=native={}/../lib",
        build_dir.display()
    );

    println!("cargo:root={}", out_dir.display());

    // Linkage directives to pull in jemalloc and its dependencies.
    //
    // On some platforms we need to be sure to link in `pthread` which jemalloc
    // depends on, and specifically on android we need to also link to libgcc.
    // Currently jemalloc is compiled with gcc which will generate calls to
    // intrinsics that are libgcc specific (e.g. those intrinsics aren't present in
    // libcompiler-rt), so link that in to get that support.
    if target.contains("windows") {
        println!("cargo:rustc-link-lib=static=pkmalloc");
    } else {
        println!("cargo:rustc-link-lib=static=pkmalloc_pic");
    }
    println!("cargo:rustc-link-search=native={}/lib", build_dir.display());
    if target.contains("android") {
        println!("cargo:rustc-link-lib=gcc");
    } else if !target.contains("windows") {
        println!("cargo:rustc-link-lib=pthread");
        println!("cargo:rustc-flags=-l dylib=stdc++");
    }
}

fn run(cmd: &mut Command) {
    println!("running: {:?}", cmd);
    let status = match cmd.status() {
        Ok(status) => status,
        Err(e) => panic!("failed to execute command: {}", e),
    };
    if !status.success() {
        panic!(
            "command did not execute successfully: {:?}\n\
             expected success, got: {}",
            cmd, status
        );
    }
}
