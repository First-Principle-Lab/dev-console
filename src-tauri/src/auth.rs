/// System authentication for an explicit manual unlock.
///
/// Reading a credential out of an already-unlocked keyring is *not* authentication, so a
/// manual lock can only be cleared by a real polkit-checked helper. The helper and its policy
/// ship with the app but stay inactive until the documented privileged install step is run;
/// nothing here edits PAM or locks the user's shared login collection.
const POLICY_PATH: &str = "/usr/share/polkit-1/actions/ai.codemate.devconsole.policy";

pub fn helper_installed() -> bool {
    std::path::Path::new("/usr/libexec/dev-console-auth-helper").exists()
        && std::path::Path::new(POLICY_PATH).exists()
        && std::path::Path::new("/usr/bin/pkexec").exists()
}

/// True when a real authentication prompt can actually be presented on this system.
pub fn mechanism_available() -> bool {
    helper_installed()
}

pub fn unavailable_reason() -> &'static str {
    "authenticated unlock is not installed: run the documented privileged helper install step \
     (polkit policy plus dev-console-auth-helper); until then the vault can only be unlocked by \
     the automatic keyring path when it is not manually locked"
}
