fn main() {
    cgo::Build::new()
        .trimpath(true)
        .package("src/d2_sys/lib.go")
        .build("d2-sys");

    #[cfg(target_os = "macos")]
    {
        // I'm not sure why this isn't included in the go library: https://github.com/golang/go/issues/47588#issuecomment-894759321
        for link_ark in ["-framework", "CoreFoundation", "-framework", "Security"] {
            println!("cargo::rustc-link-arg={link_ark}");
        }
    }
}
