fn main() {
    cc::Build::new()
        .cpp(true)
        .file("../crypto_cpp/keccak256.cpp")
        .file("../crypto_cpp/secp256k1.cpp")
        .include("../crypto_cpp")
        .flag("-std=c++17")
        .flag("-O3")
        .flag("-fno-exceptions")
        .flag("-fno-rtti")
        .compile("tiger_crypto");

    for f in ["keccak256.cpp", "secp256k1.cpp", "tiger_crypto.h"] {
        println!("cargo:rerun-if-changed=../crypto_cpp/{f}");
    }
}
