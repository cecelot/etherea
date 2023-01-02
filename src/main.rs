use log::trace;
use std::sync::{mpsc, Arc, RwLock};
use winit::event_loop::{ControlFlow, EventLoop};
use winit_input_helper::WinitInputHelper;

const _IBM_LOGO: &[u8] = include_bytes!("../roms/ibm-logo.ch8");
const _KEYS_TEST: &[u8] = &[0xF0, 0x0A, 0xE0, 0x9E, 0x12, 0x04];
const TIMER_TEST: &[u8] = &[0xF0, 0x07, 0xF0, 0x15, 0xF0, 0x18, 0x12, 0x06];

fn main() {
    env_logger::init();

    let el = EventLoop::new();
    let mut input = WinitInputHelper::new();

    let intr = Arc::new(RwLock::new({
        let display = etherea::Display::new(&el);
        let mut intr = etherea::Interpreter::new();
        intr.attach_display(display);
        // intr.load_rom(KEYS_TEST);
        // intr.load_rom(IBM_LOGO);
        intr.load_rom(TIMER_TEST);
        intr
    }));

    let (tx, rx) = mpsc::channel();
    etherea::run(&intr, rx);

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
