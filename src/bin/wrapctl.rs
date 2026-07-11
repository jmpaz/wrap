use std::process;
use std::str::FromStr;
use wrap::Format;

#[cfg(target_os = "macos")]
use std::io::Write;
#[cfg(not(target_os = "macos"))]
use std::io::{Read, Write};
#[cfg(not(target_os = "macos"))]
use std::os::unix::net::UnixStream;
#[cfg(not(target_os = "macos"))]
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};
#[cfg(not(target_os = "macos"))]
use std::time::{Duration, SystemTime, UNIX_EPOCH};
#[cfg(target_os = "macos")]
use wrap::{transform_clipboard_for_paste, unwrap_auto};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return run_macos(std::env::args().skip(1).collect());

    #[cfg(not(target_os = "macos"))]
    run_wayland()
}

#[cfg(not(target_os = "macos"))]
fn run_wayland() -> Result<(), String> {
    let request = request_from_args(std::env::args().skip(1).collect())?;
    let socket = socket_path()?;
    let mut stream = UnixStream::connect(&socket)
        .map_err(|err| format!("failed to connect to {}: {err}", socket.display()))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|err| format!("failed to set read timeout: {err}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|err| format!("failed to set write timeout: {err}"))?;

    stream
        .write_all(request.as_bytes())
        .and_then(|_| stream.shutdown(std::net::Shutdown::Write))
        .map_err(|err| format!("failed to send request: {err}"))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|err| format!("failed to read response: {err}"))?;

    let response = response.trim_end();
    if let Some(message) = response.strip_prefix("OK ") {
        println!("{message}");
        Ok(())
    } else if response == "OK" {
        Ok(())
    } else if let Some(message) = response.strip_prefix("ERR ") {
        Err(message.to_string())
    } else {
        Err(format!("unexpected daemon response: {response}"))
    }
}

#[cfg(target_os = "macos")]
fn run_macos(args: Vec<String>) -> Result<(), String> {
    match macos_request_from_args(&args)? {
        MacosRequest::Status => macos_status(),
        MacosRequest::UnwrapPaste => macos_paste(unwrap_auto),
        MacosRequest::Paste(format) => {
            macos_paste(|content| transform_clipboard_for_paste(content, format))
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug, Eq, PartialEq)]
enum MacosRequest {
    Status,
    Paste(Format),
    UnwrapPaste,
}

#[cfg(target_os = "macos")]
fn macos_request_from_args(args: &[String]) -> Result<MacosRequest, String> {
    match args {
        [command] if command == "status" => Ok(MacosRequest::Status),
        [command] if command == "unwrap-paste" => Ok(MacosRequest::UnwrapPaste),
        [command, format] if command == "paste" => {
            Ok(MacosRequest::Paste(Format::from_str(format)?))
        }
        _ => Err(usage()),
    }
}

#[cfg(target_os = "macos")]
fn macos_paste(transform: impl FnOnce(&str) -> String) -> Result<(), String> {
    let content = Command::new("/usr/bin/pbpaste")
        .output()
        .map_err(|err| format!("failed to read the macOS pasteboard: {err}"))?;

    if !content.status.success() {
        return Err("failed to read the macOS pasteboard".to_string());
    }

    let mut copy = Command::new("/usr/bin/pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to write the macOS pasteboard: {err}"))?;
    copy.stdin
        .take()
        .ok_or_else(|| "failed to open pbcopy stdin".to_string())?
        .write_all(transform(&String::from_utf8_lossy(&content.stdout)).as_bytes())
        .map_err(|err| format!("failed to write the macOS pasteboard: {err}"))?;

    if !copy
        .wait()
        .map_err(|err| format!("failed to wait for pbcopy: {err}"))?
        .success()
    {
        return Err("failed to write the macOS pasteboard".to_string());
    }

    let status = Command::new("/usr/bin/open")
        .args(["-g", "hammerspoon://wrap-paste"])
        .status()
        .map_err(|err| format!("failed to dispatch paste to Hammerspoon: {err}"))?;
    if !status.success() {
        return Err("failed to dispatch paste to Hammerspoon".to_string());
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_status() -> Result<(), String> {
    let status = Command::new("/usr/bin/pgrep")
        .args(["-x", "Hammerspoon"])
        .status()
        .map_err(|err| format!("failed to check Hammerspoon: {err}"))?;

    if status.success() {
        println!("hammerspoon: running");
        Ok(())
    } else {
        Err("Hammerspoon is not running; activate the managed Wrap agent and grant Accessibility permission".to_string())
    }
}

#[cfg(not(target_os = "macos"))]
fn request_from_args(args: Vec<String>) -> Result<String, String> {
    match args.as_slice() {
        [command] if command == "status" => Ok("STATUS\n".to_string()),
        [command] if command == "emit-paste" => Ok(format!("EMIT_PASTE {}\n", now_ms())),
        [command] if command == "paste-stdin" => paste_stdin_request(),
        [command] if command == "unwrap-paste" => Ok(format!("UNWRAP_PASTE {}\n", now_ms())),
        [command, format] if command == "paste" => {
            let format = Format::from_str(format)?;
            Ok(format!("PASTE {} {}\n", format.as_wire(), now_ms()))
        }
        _ => Err(usage()),
    }
}

#[cfg(not(target_os = "macos"))]
fn paste_stdin_request() -> Result<String, String> {
    let mut content = String::new();
    std::io::stdin()
        .read_to_string(&mut content)
        .map_err(|err| format!("failed to read stdin: {err}"))?;
    Ok(format!(
        "PASTE_TEXT {} {}\n{}",
        now_ms(),
        content.len(),
        content
    ))
}

#[cfg(not(target_os = "macos"))]
fn socket_path() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("WRAPD_SOCKET") {
        return Ok(PathBuf::from(path));
    }

    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .map_err(|_| "XDG_RUNTIME_DIR is not set and WRAPD_SOCKET was not provided".to_string())?;

    Ok(PathBuf::from(runtime_dir).join("wrap/wrapd.sock"))
}

fn usage() -> String {
    "Usage:\n  wrapctl paste [md|xml]\n  wrapctl paste-stdin\n  wrapctl emit-paste\n  wrapctl unwrap-paste\n  wrapctl status".to_string()
}

#[cfg(not(target_os = "macos"))]
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    use super::request_from_args;

    #[test]
    fn emit_paste_builds_timestamped_request() {
        let request = request_from_args(vec!["emit-paste".to_string()]).unwrap();

        assert!(request.starts_with("EMIT_PASTE "));
        assert!(request.ends_with('\n'));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn routes_darwin_commands() {
        assert_eq!(
            macos_request_from_args(&["paste".to_string(), "md".to_string()]),
            Ok(MacosRequest::Paste(Format::Markdown))
        );
        assert_eq!(
            macos_request_from_args(&["unwrap-paste".to_string()]),
            Ok(MacosRequest::UnwrapPaste)
        );
        assert_eq!(
            macos_request_from_args(&["status".to_string()]),
            Ok(MacosRequest::Status)
        );
    }
}
