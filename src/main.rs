//! 桌面端入口。wasm 入口是库里的 `start`，构建时加 `--lib`。

#[cfg(target_family = "wasm")]
compile_error!(
    "wasm 只构建库，避免和 start 同时成为入口：cargo build --target wasm32-unknown-unknown --lib"
);

fn main() {
    gpui_zoom_test::run();
}
