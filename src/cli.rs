use clap::{Parser, ValueEnum};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// The path to the ROM
    pub path: String,

    /// The number of instructions to execute per second
    #[arg(short, long)]
    pub ips: Option<u64>,

    /// Verbosity of debug logging
    #[arg(short, long, value_enum)]
    log_level: Option<LogLevel>,
}

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
