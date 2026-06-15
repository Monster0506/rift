use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
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

pub fn session_path(pid: u32) -> PathBuf {
    data_dir().join("sessions").join(format!("{}.json", pid))
}

pub fn write(info: &SessionInfo, path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(info)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}

pub fn read(path: &Path) -> anyhow::Result<SessionInfo> {
    let data = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&data)?)
}

pub fn remove(path: &Path) {
    let _ = std::fs::remove_file(path);
}

pub fn is_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        Path::new(&format!("/proc/{}", pid)).exists()
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
            .args(["/FI", &format!("PID eq {}", pid), "/NH", "/FO", "CSV"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&format!(",\"{}\",", pid)))
            .unwrap_or(false)
    }
}

pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).expect("os rng failed");
    let mut s = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write;
        write!(s, "{:02x}", b).unwrap();
    }
    s
}

/// Print the newest live session as a single compact JSON line, then exit.
/// Exits 1 if no live session exists. Called via SSH by a remote client.
pub fn print_newest() -> ! {
    match find_local().and_then(|p| read(&p)) {
        Ok(info) => {
            println!("{}", serde_json::to_string(&info).unwrap());
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("rift: {e}");
            std::process::exit(1);
        }
    }
}

/// Return the path to the newest live local session, or an error.
pub fn find_local() -> anyhow::Result<PathBuf> {
    let sessions_dir = data_dir().join("sessions");

    let mut entries: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(&sessions_dir)
        .map_err(|_| anyhow::anyhow!("no sessions directory -- is a daemon running?"))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
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

    if entries.is_empty() {
        anyhow::bail!("no live rift daemon found -- start one with: rift --daemon [file]");
    }

    entries.sort_by_key(|(t, _)| *t);
    Ok(entries.pop().unwrap().1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn write_and_read_session() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("12345.json");
        let info = SessionInfo {
            pid: 12345,
            host: "127.0.0.1".into(),
            port: 7619,
            token: "tok".into(),
        };
        write(&info, &path).unwrap();
        let back = read(&path).unwrap();
        assert_eq!(back.pid, 12345);
        assert_eq!(back.token, "tok");
    }

    #[test]
    fn generate_token_length() {
        let t = generate_token();
        assert_eq!(t.len(), 64);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn is_alive_current_process() {
        assert!(is_alive(std::process::id()));
    }

    #[test]
    fn is_alive_bogus_pid() {
        assert!(!is_alive(2_000_000));
    }

    #[test]
    fn session_path_contains_pid() {
        let path = session_path(99999);
        assert!(path.to_string_lossy().ends_with("99999.json"));
    }

    #[test]
    fn generate_token_is_unique() {
        assert_ne!(generate_token(), generate_token());
    }

    #[test]
    fn write_overwrites_existing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("99.json");
        let info1 = SessionInfo {
            pid: 99,
            host: "127.0.0.1".into(),
            port: 1234,
            token: "old".into(),
        };
        write(&info1, &path).unwrap();
        let info2 = SessionInfo {
            pid: 99,
            host: "127.0.0.1".into(),
            port: 1234,
            token: "new".into(),
        };
        write(&info2, &path).unwrap();
        let back = read(&path).unwrap();
        assert_eq!(back.token, "new");
    }
}
