use std::io::{self, IsTerminal, Read};
use std::process;
use std::str::FromStr;
use wrap::{unwrap_auto, wrap_content, Format};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "md".to_string());

    let mut content = String::new();
    if io::stdin().is_terminal() {
        return Err(usage());
    }
    io::stdin()
        .read_to_string(&mut content)
        .map_err(|err| format!("failed to read stdin: {err}"))?;

    match command.as_str() {
        "unwrap" => {
            print!("{}", unwrap_auto(&content));
            Ok(())
        }
        value => {
            let format = Format::from_str(value)?;
            print!("{}", wrap_content(&content, format));
            Ok(())
        }
    }
}

fn usage() -> String {
    "Usage:\n  wrap [md|xml] < input\n  wrap unwrap < input".to_string()
}
