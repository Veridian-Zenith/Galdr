fn main() {
    // Override CFLAGS to avoid native AVX instructions (for QEMU compatibility)
    // SAFETY: build scripts run single-threaded before any parallel code
    unsafe { std::env::set_var("CFLAGS", "-march=x86-64 -O2") };

    cc::Build::new()
        .file("src/libc.c")
        .compile("libc_shim");

    println!("cargo:rustc-link-arg=-nostartfiles");
    println!("cargo:rustc-link-arg=-static");
}
