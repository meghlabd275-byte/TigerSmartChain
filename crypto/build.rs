fn main() {
    cc::Build::new()
        .cpp(true)
        .file("../crypto_cpp/keccak256.cpp")
        .file("../crypto_cpp/ecdsa.cpp")
        .flag("-std=c++17")
        .flag("-O3")
        .compile("tiger_crypto");

    println!("cargo:rerun-if-changed=../crypto_cpp/keccak256.cpp");
    println!("cargo:rerun-if-changed=../crypto_cpp/ecdsa.cpp");
}
