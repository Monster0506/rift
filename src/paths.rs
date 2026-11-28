use std::path::PathBuf;

#[cfg(test)]
pub(crate) fn rift_dir() -> PathBuf {
    std::env::temp_dir().join(format!("rift-test-{}", std::process::id()))
}

#[cfg(not(test))]
pub(crate) fn rift_dir() -> PathBuf {
    if cfg!(windows) {
        std::env::var("APPDATA")
            .map_or_else(|_| PathBuf::from("."), PathBuf::from)
            .join("rift")
    } else {
        std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map_or_else(
                || PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config"),
                PathBuf::from,
            )
            .join("rift")
    }
}
