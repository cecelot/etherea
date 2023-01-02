use etherea::cli;
use log::error;

fn main() {
    let cli = cli::init();
    match cli.command {
        cli::Commands::Run { path, ips } => cli::run(&path, ips),
        cli::Commands::Disassemble { path, output_file } => cli::disassemble(&path, output_file)
            .unwrap_or_else(|e| {
                error!("{}", e);
                std::process::exit(1);
            }),
    }
}
