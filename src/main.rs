use etherea::cli;
use log::{error, trace};
use std::{fmt, fs, io, path::Path, sync::mpsc};
use winit::event_loop::{ControlFlow, EventLoop};
use winit_input_helper::WinitInputHelper;

fn main() {
    let cli = cli::init();
    let el = EventLoop::new();
    let mut input = WinitInputHelper::new();

    let rom = read(cli.path).unwrap_or_else(|err| {
        error!("{}", err);
        std::process::exit(1);
    });

    let (tx, rx) = mpsc::channel();
    etherea::run(&rom, &el, rx);

    el.run(move |event, _, cf| {
        if input.update(&event) {
            if input.quit() {
                *cf = ControlFlow::Exit;
                return;
            }

            for &key in etherea::input::KEYMAP.keys() {
                if input.key_pressed(key) {
                    trace!("Sending {:?} to interpreter", key);
                    tx.send(key).unwrap();
                    break;
                }
            }
        }
    });
}

fn read<P: AsRef<Path> + fmt::Display>(path: P) -> Result<Vec<u8>, String> {
    let err = |_: io::Error| format!("Could not read file: '{}'", path);
    let path = fs::canonicalize(&path).map_err(err)?;
    fs::read(path).map_err(err)
}
