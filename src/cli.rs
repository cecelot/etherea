use clap::{Parser, Subcommand, ValueEnum};
use log::error;
use std::{
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

/// The etherea CLI.
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Verbosity of debug logging
    #[arg(short, long, value_enum)]
    log_level: Option<LogLevel>,
}

/// Possible commands to run.
#[derive(Subcommand)]
pub enum Commands {
    /// Runs a ROM.
    Run {
        /// The path to the ROM
        path: String,

        /// The number of instructions to execute per second
        #[arg(short, long)]
        ips: Option<u64>,
    },
    /// Disassembles a ROM.
    Disassemble {
        /// The path to the ROM
        path: PathBuf,

        /// Where to output the disassembled ROM
        #[arg(short, long)]
        output_file: Option<PathBuf>,
    },
}

/// The logging level passed to [`env_logger`](env_logger).
#[derive(Copy, Clone, ValueEnum)]
enum LogLevel {
    Info,
    Debug,
    Trace,
    Error,
}

impl ToString for LogLevel {
    fn to_string(&self) -> String {
        match self {
            Self::Info => "info".into(),
            Self::Debug => "debug".into(),
            Self::Trace => "trace".into(),
            Self::Error => "error".into(),
        }
    }
}

/// Parses the command-line args and configures the logging level.
#[must_use]
pub fn init() -> Cli {
    let cli = Cli::parse();
    std::env::set_var(
        "RUST_LOG",
        format!(
            "etherea={}",
            cli.log_level.unwrap_or(LogLevel::Error).to_string()
        ),
    );

    env_logger::init();

    cli
}

/// Runs the ROM at `path` with the provided `ips`.
pub fn run(path: &String, ips: Option<u64>) {
    let rom = read(path).unwrap_or_else(|err| {
        error!("{}", err);
        std::process::exit(1);
    });

    crate::run(&rom, ips.unwrap_or(700));
}

/// Disassembles the ROM at `input_path`.
///
/// # Errors
/// This function will error if `output_file` is not a file or the file at `input_path`
/// cannot be read.
pub fn disassemble(input_path: &PathBuf, output_file: Option<PathBuf>) -> Result<(), io::Error> {
    if let Some(mut f) = output_file.clone() {
        if f.extension().is_none() {
            error!("{} is not a file", f.display());
            std::process::exit(1);
        }
        f.pop();
        fs::create_dir_all(f)?;
    }

    let path = output_file.unwrap_or_else(|| PathBuf::from("output.txt"));
    let mut file = fs::File::create(&path)?;
    let rom = fs::read(input_path)?;

    writeln!(file, "== {} ==", path.display())?;
    for chunk in rom.chunks_exact(2) {
        let inst = crate::Instruction::from(u16::from_be_bytes([chunk[0], chunk[1]]));
        writeln!(file, "{inst:?}")?;
    }

    file.flush()?;

    println!("Wrote disassembled ROM to {}", path.display());

    Ok(())
}

/// Reads the file at `path` as bytes, returning an error if it could not be read.
fn read<P: AsRef<Path> + fmt::Display>(path: P) -> Result<Vec<u8>, String> {
    let err = |_: io::Error| format!("Could not read file: '{path}'");
    let path = fs::canonicalize(&path).map_err(err)?;
    fs::read(path).map_err(err)
}
