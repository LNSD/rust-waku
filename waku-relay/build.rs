use std::env;
use std::error::Error;
use std::process::{exit, Command};

fn main() -> Result<(), Box<dyn Error>> {
    let out_dir = env::var_os("OUT_DIR").unwrap();

    let status = Command::new("buf")
        .arg("generate")
        .arg("https://github.com/LNSD/waku-proto.git#branch=rust-waku")
        .arg("--path")
        .arg("waku/relay")
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
