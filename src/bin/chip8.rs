use winit::{event::Event, event_loop::EventLoop};

fn main() {
    let event_loop = EventLoop::new();
    let mut intr = chip8::Interpreter::new(&event_loop);

    event_loop.run(move |event, _, _| {
        if let Event::RedrawRequested(_) = event {
            if let Err(e) = intr.render() {
                eprintln!("error writing to screen: {:?}", e);
            }
        }
        intr.get_window().request_redraw();
    });
}
