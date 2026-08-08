fn main() {
    // Linux/Wayland: 确保 -lwayland-client 在链接行末尾（不被 --as-needed 丢弃）。
    // macOS 不需要这些 flag，Apple ld64 不认识 --no-as-needed。
    #[cfg(target_os = "linux")]
    {
        println!("cargo:rustc-link-arg=-Wl,--no-as-needed");
        println!("cargo:rustc-link-arg=-lwayland-client");
        println!("cargo:rustc-link-arg=-Wl,--as-needed");
    }
}
