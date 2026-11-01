fn main() {
    // WebKitGTK 2.54's dmabuf renderer commits Wayland buffers without a drm-syncobj acquire
    // timeline, which Hyprland treats as a protocol error and answers by killing the client
    // (observed as `wl_display.error(wp_linux_drm_syncobj_surface_v1, 4, "Missing acquire
    // timeline")` the moment a window maps). Until the upstream fix lands, keep the stable
    // shared-memory compositor on Linux; an explicitly set value always wins.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    dev_console_lib::run()
}
