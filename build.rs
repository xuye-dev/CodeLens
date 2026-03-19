fn main() {
    // 仅在启用 embedding feature 时编译 glibc 兼容符号桩
    if std::env::var("CARGO_FEATURE_EMBEDDING").is_ok() {
        cc::Build::new().file("compat.c").compile("compat");
    }
}
