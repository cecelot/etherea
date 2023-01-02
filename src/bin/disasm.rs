use clap::Parser;
use log::{error, info};
use std::{fs, io::Write, path::PathBuf};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// The path to the ROM
    path: PathBuf,

    /// Where to output the disassembled ROM
    #[arg(short, long)]
    output_file: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::env::set_var("RUST_LOG", "info");
    env_logger::init();

    let cli = Cli::parse();

    if let Some(mut f) = cli.output_file.clone() {
        if f.extension().is_none() {
            error!("{} is not a file", f.display());
            std::process::exit(1);
        }
        f.pop();
        fs::create_dir_all(f)?;
    }

    let path = cli.output_file.unwrap_or(PathBuf::from("output.txt"));
    let mut file = fs::File::create(&path)?;
    let rom = fs::read(&cli.path)?;

    writeln!(file, "== {} ==", cli.path.display())?;
    for chunk in rom.chunks_exact(2) {
        let inst = etherea::Instruction::from(u16::from_be_bytes([chunk[0], chunk[1]]));
        writeln!(file, "{:?}", inst)?;
    }

    file.flush()?;

    info!("Wrote disassembled ROM to {}", path.display());

    Ok(())
}
