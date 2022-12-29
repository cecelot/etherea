use pixels::{Pixels, SurfaceTexture};
use std::ops::RangeInclusive;
use winit::event_loop::EventLoop;
use winit::{
    dpi::LogicalSize,
    window::{Window, WindowBuilder},
};

const MEMORY_SIZE: usize = 4096;
const REGISTER_COUNT: usize = 16;

const FONT_RANGE: RangeInclusive<usize> = 0x50..=0x9F;
const FONT: &[u8] = &[
    0xF0, 0x90, 0x90, 0x90, 0xF0, // 0
    0x20, 0x60, 0x20, 0x20, 0x70, // 1
    0xF0, 0x10, 0xF0, 0x80, 0xF0, // 2
    0xF0, 0x10, 0xF0, 0x10, 0xF0, // 3
    0x90, 0x90, 0xF0, 0x10, 0x10, // 4
    0xF0, 0x80, 0xF0, 0x10, 0xF0, // 5
    0xF0, 0x80, 0xF0, 0x90, 0xF0, // 6
    0xF0, 0x10, 0x20, 0x40, 0x40, // 7
    0xF0, 0x90, 0xF0, 0x90, 0xF0, // 8
    0xF0, 0x90, 0xF0, 0x10, 0xF0, // 9
    0xF0, 0x90, 0xF0, 0x90, 0x90, // A
    0xE0, 0x90, 0xE0, 0x90, 0xE0, // B
    0xF0, 0x80, 0x80, 0x80, 0xF0, // C
    0xE0, 0x90, 0x90, 0x90, 0xE0, // D
    0xF0, 0x80, 0xF0, 0x80, 0xF0, // E
    0xF0, 0x80, 0xF0, 0x80, 0x80, // F
];

#[derive(Debug)]
#[allow(dead_code)]
pub struct Interpreter {
    i: u16,                          // Index register
    stack: Vec<u16>,                 // Stack
    memory: [u8; MEMORY_SIZE],       // Memory
    display: Display,                // Display
    delay: u8,                       // Delay timer
    sound: u8,                       // Sound timer
    registers: [u8; REGISTER_COUNT], // Variable registers (V0..=VF)
}

impl Interpreter {
    pub fn new(event_loop: &EventLoop<()>) -> Self {
        let mut intr = Self {
            i: 0,
            stack: Vec::new(),
            memory: [0; MEMORY_SIZE],
            display: Display::new(event_loop),
            delay: 0,
            sound: 0,
            registers: [0; REGISTER_COUNT],
        };
        for location in FONT_RANGE {
            intr.memory[location] = FONT[location - FONT_RANGE.start()];
        }
        intr
    }

    pub fn get_window(&self) -> &Window {
        &self.display.window
    }

    pub fn render(&mut self) -> Result<(), pixels::Error> {
        self.draw();
        self.display.pixels.render()
    }

    fn draw(&mut self) {
        let frame = self.display.pixels.get_frame_mut();
        for (pixel, scratch_pixel) in frame
            .chunks_exact_mut(4)
            .zip(self.display.scratch_pixels.chunks_exact(4))
        {
            for i in 0..4 {
                pixel[i] = scratch_pixel[i];
            }
        }
    }
}

const WIDTH: usize = 64;
const HEIGHT: usize = 32;

#[derive(Debug)]
struct Display {
    scratch_pixels: [u8; WIDTH * HEIGHT],
    window: Window,
    pixels: Pixels,
}

impl Display {
    fn new(event_loop: &EventLoop<()>) -> Self {
        let window = {
            let size = LogicalSize::new(WIDTH as u32, HEIGHT as u32);
            let scaled_size = LogicalSize::new(WIDTH as f64 * 10.0, HEIGHT as f64 * 10.0);
            WindowBuilder::new()
                .with_title("CHIP-8")
                .with_inner_size(scaled_size)
                .with_min_inner_size(size)
                .build(&event_loop)
                .unwrap()
        };

        let pixels = {
            let size = window.inner_size();
            let texture = SurfaceTexture::new(size.width, size.height, &window);
            Pixels::new(WIDTH as u32, HEIGHT as u32, texture).unwrap()
        };

        Self {
            scratch_pixels: [0; WIDTH * HEIGHT],
            window,
            pixels,
        }
    }
}
