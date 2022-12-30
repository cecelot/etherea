use log::trace;
use std::{
    sync::{mpsc, Arc, RwLock},
    thread,
};
use winit::{
    event::VirtualKeyCode,
    event_loop::{ControlFlow, EventLoop},
};
use winit_input_helper::WinitInputHelper;

const _IBM_LOGO: &[u8] = include_bytes!("../roms/ibm-logo.ch8");
const KEYS_TEST: &[u8] = &[0xF0, 0x0A, 0xE0, 0x9E, 0x12, 0x04];

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new();
    let mut input = WinitInputHelper::new();

    let intr = Arc::new(RwLock::new({
        let display = chip8::Display::new(&event_loop);
        let mut intr = chip8::Interpreter::new();
        intr.attach_display(display);
        intr.load_rom(KEYS_TEST.to_vec());
        // intr.load_rom(IBM_LOGO.to_vec());
        intr
    }));

    let (tx, rx) = mpsc::channel::<VirtualKeyCode>();

    let intr1 = Arc::clone(&intr);
    thread::spawn(move || {
        intr1.write().unwrap().execute(rx);
    });

    event_loop.run(move |event, _, control_flow| {
        if input.update(&event) {
            if input.quit() {
                *control_flow = ControlFlow::Exit;
                return;
            }

            for &key in chip8::input::KEYMAP.keys() {
                if input.key_pressed(key) {
                    trace!("Sending {:?} to interpreter", key);
                    tx.send(key).unwrap();
                    break;
                }
            }
        }
    });
}
