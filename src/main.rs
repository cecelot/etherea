use std::sync::{Arc, RwLock};
use std::thread;
use winit::event_loop::{ControlFlow, EventLoop};
use winit_input_helper::WinitInputHelper;

const IBM_LOGO: &[u8] = include_bytes!("../roms/ibm-logo.ch8");

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new();
    let mut input = WinitInputHelper::new();

    let intr = Arc::new(RwLock::new({
        let display = chip8::Display::new(&event_loop);
        let mut intr = chip8::Interpreter::new();
        intr.attach_display(display);
        intr.load_rom(IBM_LOGO.to_vec());
        intr
    }));

    let intr1 = Arc::clone(&intr);
    thread::spawn(move || {
        intr1.write().unwrap().execute();
    });

    event_loop.run(move |event, _, control_flow| {
        if input.update(&event) {
            if input.quit() {
                *control_flow = ControlFlow::Exit;
                return;
            }
        }
    });
}
