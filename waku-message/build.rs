use std::env;
use std::error::Error;
use std::process::{Command, exit};

fn main() -> Result<(), Box<dyn Error>> {
    let out_dir = env::var_os("OUT_DIR").unwrap();

    let status = Command::new("buf")
        .arg("generate")
        .arg("https://github.com/vacp2p/waku.git")
        .arg("--path")
        .arg("waku/message")
        .arg("--output")
        .arg(out_dir)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .unwrap();

    if !status.success() {
        exit(status.code().unwrap_or(-1))
    }

    Ok(())
}
