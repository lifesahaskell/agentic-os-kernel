fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-arg-bins=-T{dir}/../link.ld");
    println!("cargo:rustc-link-arg-bins=--no-dynamic-linker");
    println!("cargo:rustc-link-arg-bins=-zmax-page-size=4096");
    println!("cargo:rerun-if-changed=../link.ld");
}
