use clap::{Parser, ValueEnum};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// The path to the ROM
    pub path: String,

    /// Verbosity of debug logging
    #[arg(short, long, value_enum)]
    debug: Option<DebugMode>,
}

#[derive(Copy, Clone, ValueEnum)]
enum DebugMode {
    Info,
    Debug,
    Trace,
    Error,
}

impl ToString for DebugMode {
    fn to_string(&self) -> String {
        match self {
            Self::Info => "info".into(),
            Self::Debug => "debug".into(),
            Self::Trace => "trace".into(),
            Self::Error => "error".into(),
        }
    }
}

pub fn init() -> Cli {
    let cli = Cli::parse();
    std::env::set_var(
        "RUST_LOG",
        format!(
            "etherea={}",
            cli.debug.unwrap_or(DebugMode::Error).to_string()
        ),
    );

    env_logger::init();

    cli
}
