use log::{debug, error, info, trace};
use pixels::{Pixels, SurfaceTexture};
use std::fmt;
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
    pc: u16,                         // Program counter
    stack: Vec<u16>,                 // Stack
    memory: [u8; MEMORY_SIZE],       // Memory
    display: Display,                // Display
    delay: u8,                       // Delay timer
    sound: u8,                       // Sound timer
    registers: [u8; REGISTER_COUNT], // Variable registers (V0..=VF)
    rom: Vec<u8>,
}

impl Interpreter {
    fn decode(&mut self) -> Instruction {
        Instruction::from(self.fetch())
    }

    fn draw(&mut self) {
        let frame = self.display.pixels.get_frame_mut();
        for (pixel, scratch_pixel) in frame
            .chunks_exact_mut(4)
            .zip(self.display.scratch_pixels.chunks_exact(4))
        {
            pixel.copy_from_slice(scratch_pixel);
        }
    }

    fn fetch(&mut self) -> u16 {
        if self.pc as usize >= self.rom.len() {
            return 0;
        }
        let inst = u16::from_be_bytes([self.rom[self.pc as usize], self.rom[self.pc as usize + 1]]);
        self.pc += 2;
        inst
    }

    pub fn get_window(&self) -> &Window {
        &self.display.window
    }

    pub fn load_rom(&mut self, rom: Vec<u8>) {
        self.i = 0;
        self.pc = 0;
        self.stack = Vec::new();
        self.memory = [0; MEMORY_SIZE];
        self.delay = 0;
        self.sound = 0;
        self.registers = [0; REGISTER_COUNT];
        self.rom = rom;

        self.memory[FONT_RANGE].copy_from_slice(FONT);
        self.memory[0x200..0x200 + self.rom.len()].copy_from_slice(&self.rom);
        info!("Loaded ROM [size: {}]", self.rom.len());
    }

    pub fn new(event_loop: &EventLoop<()>) -> Self {
        let mut intr = Self {
            i: 0,
            pc: 0,
            stack: Vec::new(),
            memory: [0; MEMORY_SIZE],
            display: Display::new(event_loop),
            delay: 0,
            sound: 0,
            registers: [0; REGISTER_COUNT],
            rom: Vec::new(),
        };
        intr.memory[FONT_RANGE].copy_from_slice(FONT);
        info!("Font status [loaded: {}]", &intr.memory[FONT_RANGE] == FONT);
        intr
    }

    pub fn render(&mut self) -> Result<(), pixels::Error> {
        self.draw();
        self.display.pixels.render()
    }

    pub fn run(&mut self) {
        loop {
            let inst = self.decode();
            debug!("Processing instruction [{:?}]", inst);
            match inst.nibbles[..] {
                [0, 0, 0xE, 0] => {
                    self.display.scratch_pixels = [0; WIDTH * HEIGHT * 4];
                    self.render().unwrap();
                    debug!("Cleared screen");
                }
                [1, n1, n2, n3] => {
                    let pc = u16::from_be_bytes([n1, (n2 << 4) | n3]);
                    self.pc = pc;
                    debug!("Jumped PC to {pc}");
                }
                [6, register, n1, n2] => {
                    let value = (n1 << 4) | n2;
                    self.registers[register as usize] = value;
                    debug!("Set register V{register:01X} to {value}")
                }
                [7, register, n1, n2] => {
                    let value = (n1 << 4) | n2;
                    self.registers[register as usize] += value;
                    debug!("Added {value} to register V{register:01X}");
                }
                [0xA, n1, n2, n3] => {
                    let value = u16::from_be_bytes([n1, (n2 << 4) | n3]);
                    self.i = value;
                    debug!("Set index register I to {value}");
                }
                [0xD, vx, vy, height] => {
                    let x = self.registers[vx as usize] % WIDTH as u8;
                    let y = self.registers[vy as usize] % HEIGHT as u8;
                    trace!("x: {x} height: {height}");
                    self.registers[0xF] = 0;
                    let sprites = &self.memory[self.i as usize..];
                    for (idx, y) in (y..y + height).enumerate() {
                        let sprite = sprites[idx];
                        for (n, x) in (x..x + 8).enumerate() {
                            let lit = set(7 - n as u8, sprite);
                            trace!("Drawing pixel [on: {}] [idx: {idx}] at ({x}, {y})", lit);
                            let set = self.display.write_at(x, y, [0xFF, 0xFF, 0xFF, 0xFF], lit);
                            if set {
                                self.registers[0xF] = 1;
                            }
                        }
                    }
                    self.render().unwrap();
                }
                _ => {
                    error!("Unknown opcode");
                    return;
                }
            }
        }
    }
}

const fn set(n: u8, bits: u8) -> bool {
    (bits & (1 << n)) != 0
}

const WIDTH: usize = 64;
const HEIGHT: usize = 32;

#[derive(Debug)]
struct Display {
    scratch_pixels: [u8; WIDTH * HEIGHT * 4], // RGBA
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
                .build(event_loop)
                .unwrap()
        };

        let pixels = {
            let size = window.inner_size();
            let texture = SurfaceTexture::new(size.width, size.height, &window);
            Pixels::new(WIDTH as u32, HEIGHT as u32, texture).unwrap()
        };

        Self {
            scratch_pixels: [0; WIDTH * HEIGHT * 4],
            window,
            pixels,
        }
    }

    fn write_at(&mut self, x: u8, y: u8, rgba: [u8; 4], on: bool) -> bool {
        let x = x as usize;
        let y = y as usize;
        let idx = (y * WIDTH + x) * 4;
        let pixels = if on { rgba } else { [0x0, 0x0, 0x0, 0x0] };
        let set = &self.scratch_pixels[idx..idx + 4] == &[0xFF, 0xFF, 0xFF, 0xFF];
        self.scratch_pixels[idx..idx + 4].copy_from_slice(&pixels);
        set
    }
}

#[derive(PartialEq)]
struct Instruction {
    nibbles: Vec<u8>,
}

impl From<u16> for Instruction {
    fn from(inst: u16) -> Self {
        Self {
            nibbles: inst
                .to_be_bytes()
                .iter()
                .flat_map(|b| vec![(b & 0xF0) >> 4, (b & 0xF)])
                .collect(),
        }
    }
}

impl fmt::Debug for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for nibble in self.nibbles.iter() {
            write!(f, "{:X}", nibble)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitops() {
        let val = 0b00101110; // 46
        let inst = Instruction::from(val);
        assert_eq!(
            inst,
            Instruction {
                nibbles: vec![0, 0, 0b0010, 0b1110]
            }
        );
    }
}
