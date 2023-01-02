use log::error;
use std::sync::{Arc, RwLock};
use std::thread;
use winit::{event::Event, event_loop::EventLoop};

fn main() {
    env_logger::init();

    let event_loop = EventLoop::new();
    let intr = Arc::new(RwLock::new(chip8::Interpreter::new(&event_loop)));

    intr.write()
        .unwrap()
        .load_rom(include_bytes!("../roms/ibm-logo.ch8").to_vec());

    let intr1 = Arc::clone(&intr);
    thread::spawn(move || {
        intr1.write().unwrap().run();
    });

    event_loop.run(move |event, _, _| {
        if let Event::RedrawRequested(_) = event {
            if let Err(e) = intr.write().unwrap().render() {
                error!("Failed to render to screen: {}", e);
            }
        }
        intr.write().unwrap().get_window().request_redraw();
    });
}
