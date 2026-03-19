fn main() {
    // 编译 glibc 兼容符号桩（解决 ONNX Runtime 在旧系统上的链接问题）
    cc::Build::new().file("compat.c").compile("compat");
}
