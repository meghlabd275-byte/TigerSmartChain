fn main() {
    cc::Build::new()
        .cpp(true)
        .file("../crypto_cpp/keccak256.cpp")
        .file("../crypto_cpp/secp256k1.cpp")
        .file("../crypto_cpp/trie.cpp")
        .include("../crypto_cpp")
        .flag("-std=c++17")
        .flag("-O3")
        .flag("-fno-exceptions")
        .flag("-fno-rtti")
        .compile("tiger_crypto");

    for f in ["keccak256.cpp", "secp256k1.cpp", "trie.cpp", "tiger_crypto.h", "trie.hpp"] {
        println!("cargo:rerun-if-changed=../crypto_cpp/{f}");
    }
}
