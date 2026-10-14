/// Placeholder adapter for the Linux Secret Service. The real adapter (and the vault
/// credential it holds) is added with the encrypted-vault checkpoint.
pub fn provider_available() -> bool {
    std::env::var("DEV_CONSOLE_KEYRING_OVERRIDE").is_ok()
        || (std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some()
            && std::path::Path::new("/usr/bin/gnome-keyring-daemon").exists())
}
