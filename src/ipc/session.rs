use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub pid: u32,
    pub host: String,
    pub port: u16,
    pub token: String,
}

pub fn data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var("APPDATA")
            .map_or_else(|_| PathBuf::from("."), PathBuf::from)
            .join("rift")
    }
    #[cfg(not(windows))]
    {
        std::env::var("XDG_DATA_HOME")
            .ok()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var("HOME").unwrap_or_default())
                    .join(".local")
                    .join("share")
            })
            .join("rift")
    }
}

#[must_use]
pub fn session_path(pid: u32) -> PathBuf {
    data_dir().join("sessions").join(format!("{pid}.json"))
}

/// Write session info as pretty JSON, creating parent directories as needed.
///
/// # Errors
/// Returns `Err` if the parent directory cannot be created, the info cannot be serialized, or the file cannot be written.
pub fn write(info: &SessionInfo, path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(info)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

/// Read session info from a JSON file.
///
/// # Errors
/// Returns `Err` if the file cannot be read or does not contain valid session JSON.
pub fn read(path: &Path) -> anyhow::Result<SessionInfo> {
    let data = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&data)?)
}

pub fn remove(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[must_use]
pub fn is_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        // /FO CSV produces "name","pid",... so match the quoted PID in the second
        // field; bare substring would let PID 12 match a line containing "123".
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&format!(",\"{pid}\",")))
            .unwrap_or(false)
    }
}

/// Generate a random 64-character hex token.
///
/// # Panics
/// Panics if the OS random number generator fails.
#[must_use]
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("os rng failed");
    let mut s = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write;
        write!(s, "{b:02x}").unwrap();
    }
    s
}

/// Print the newest live session as a single compact JSON line.
/// Returns failure if no live session exists. Called via SSH by a remote client.
///
/// # Panics
/// Panics if `SessionInfo` cannot be serialized, which does not happen in practice.
#[must_use]
pub fn print_newest() -> ExitCode {
    match find_local().and_then(|p| read(&p)) {
        Ok(info) => {
            println!("{}", serde_json::to_string(&info).unwrap());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("rift: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Return the path to the newest live local session, or an error.
///
/// # Errors
/// Returns `Err` if the sessions directory cannot be read or no live session exists.
pub fn find_local() -> anyhow::Result<PathBuf> {
    let sessions_dir = data_dir().join("sessions");

    let mut entries: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(&sessions_dir)
        .map_err(|e| anyhow::anyhow!("no sessions directory ({e}) -- is a daemon running?"))?
        .filter_map(std::result::Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let path = e.path();
            let pid = path.file_stem()?.to_str()?.parse::<u32>().ok()?;
            if !is_alive(pid) {
                return None;
            }
            let mtime = e.metadata().ok()?.modified().ok()?;
            Some((mtime, path))
        })
        .collect();

    entries.sort_by_key(|(t, _)| *t);
    let Some((_, newest)) = entries.pop() else {
        anyhow::bail!("no live rift daemon found -- start one with: rift --daemon [file]");
    };
    Ok(newest)
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod session_tests;
