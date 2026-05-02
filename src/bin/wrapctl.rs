use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process;
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use wrap::Format;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::request_from_args;

    #[test]
    fn emit_paste_builds_timestamped_request() {
        let request = request_from_args(vec!["emit-paste".to_string()]).unwrap();

        assert!(request.starts_with("EMIT_PASTE "));
        assert!(request.ends_with('\n'));
    }
}
