//! The `dev-console` executable: argument forwarding and exit code, nothing else.

fn main() {
    // No `WEBKIT_DISABLE_DMABUF_RENDERER` here, on purpose.
    //
    // `src-tauri/src/main.rs` sets it because WebKitGTK 2.54's dmabuf renderer commits Wayland
    // buffers without a drm-syncobj acquire timeline, which Hyprland answers by killing the client.
    // That default belongs to the WebKit runtime, and plan §4.6 ("Remove WebKit dependencies and the
    // WebKit-specific environment default only when the Tauri runtime is removed") is explicit that
    // it goes away with Tauri, not before: the Tauri binary is still built and still needs it, so the
    // default stays exactly where it is. Setting it from this binary would be different and wrong -
    // Rune links no WebKitGTK at all, so the variable would describe a dependency this process does
    // not have, and would hide the fact that the WebKit workaround is still load-bearing next door.
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(dev_console_desktop::run(&args));
}
