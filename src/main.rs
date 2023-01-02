use etherea::cli;
use log::error;
use std::{fmt, fs, io, path::Path};

fn main() {
    let cli = cli::init();
    let rom = read(cli.path).unwrap_or_else(|err| {
        error!("{}", err);
        std::process::exit(1);
    });

    etherea::run(&rom);
}

fn read<P: AsRef<Path> + fmt::Display>(path: P) -> Result<Vec<u8>, String> {
    let err = |_: io::Error| format!("Could not read file: '{}'", path);
    let path = fs::canonicalize(&path).map_err(err)?;
    fs::read(path).map_err(err)
}
