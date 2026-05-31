fn main() {
    // Tell Cargo to look in the "vpn" directory for libraries
    println!("cargo:rustc-link-search=native=./vpn");

    // Tell Cargo to statically link "libgovpn.a" (drop the "lib" prefix and ".a" extension)
    println!("cargo:rustc-link-lib=static=govpn");

    // Note: Depending on your OS, Go static libraries require linking some
    // standard system libraries. For Linux/macOS, we usually need these:
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-lib=framework=CoreFoundation");

    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-lib=pthread");
}

// fn main() {
// println!("cargo=../vpn");
// println!("cargo=dylib=govpn");
// }
