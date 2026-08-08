fn main() {
    // libwayland-client 的请求封装函数（wl_compositor_create_region 等）在头文件里
    // 是 static inline、不导出；我们用导出的 wl_proxy_marshal_array_flags 手动封装。
    // `-lwayland-client` 必须出现在链接行末尾，否则会被 --as-needed 提前丢弃
    // （当前 crate 的 #[link] 属性会被 rustc 放在 rlib 之前，正是被丢的那个）。
    println!("cargo:rustc-link-arg=-Wl,--no-as-needed");
    println!("cargo:rustc-link-arg=-lwayland-client");
    println!("cargo:rustc-link-arg=-Wl,--as-needed");
}
