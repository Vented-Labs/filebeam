#[cfg(unix)]
fn main() -> anyhow::Result<()> {
    use std::path::PathBuf;
    let mut arguments = std::env::args_os().skip(1);
    let home = match (arguments.next(), arguments.next()) {
        (Some(flag), Some(path)) if flag == "--home" => PathBuf::from(path),
        _ => anyhow::bail!("usage: filebeamd --home PROFILE_HOME"),
    };
    filebeam_daemon::Daemon::open(&home)?.run()
}

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    filebeam_daemon::run()
}

#[cfg(not(any(unix, windows)))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("filebeamd IPC is not implemented on this platform")
}
