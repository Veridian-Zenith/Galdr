fn main() {
    cc::Build::new().file("src/libc.c").compile("libc_shim");

    println!("cargo:rustc-link-arg=-nostartfiles");
    println!("cargo:rustc-link-arg=-static");
}
