#![deny(clippy::pedantic)]
//! A CHIP-8 interpreter.
use log::{debug, error, info, trace};
use pixels::{Pixels, SurfaceTexture};
use rand::Rng;
use std::{
    fmt,
    ops::{Deref, DerefMut},
    sync::{
        mpsc::{self, Receiver, Sender, TryRecvError},
        Arc, RwLock,
    },
    thread,
};
use winit::{
    dpi::LogicalSize,
    event::VirtualKeyCode,
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};
use winit_input_helper::WinitInputHelper;

/// Helpers for the CLI.
pub mod cli;
/// Font-related constants.
mod font;
/// Input-related constants.
pub mod input;

/// A workaround for calling [`Default`](std::default::Default) on
/// an arbitrarily sized slice. Implements [`Deref`](std::ops::Deref)
/// and [`DerefMut`](std::ops::DerefMut) for ease of use.
macro_rules! wrapper {
    ($($(#[$($attrs:meta)*])* $name:ident => $size:expr),*) => {
        $(
            $(#[$($attrs)*])*
            #[derive(Debug)]
            struct $name([u8; $size]);

            impl Default for $name {
                fn default() -> Self {
                    Self([0; $size])
                }
            }

            impl Deref for $name {
                type Target = [u8; $size];

                fn deref(&self) -> &Self::Target {
                    &self.0
                }
            }

            impl DerefMut for $name {
                fn deref_mut(&mut self) -> &mut Self::Target {
                    &mut self.0
                }
            }
        )*
    };
}

/// The entrypoint for the CHIP-8 interpreter. Creates a new interpreter and
/// starts two threads, one for the fetch/decode/execute loop and one for the
/// 60Hz timer loop. Starts the window event loop in the calling thread.
pub fn run(rom: &[u8], ips: u64) {
    let el = EventLoop::new();

    let intr = Arc::new(RwLock::new({
        let display = Display::new(&el);
        let mut intr = Interpreter::new();
        intr.attach_display(display);
        intr.with_ips(ips);
        intr.load_rom(rom);
        intr
    }));

    let (tx, rx) = mpsc::channel();

    Interpreter::main(Arc::clone(&intr), rx);
    Interpreter::timers(&intr);
    Interpreter::ui(el, tx);
}

/// The CHIP-8 interpreter state.
/// [Specifications](https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#specifications).
#[derive(Debug, Default)]
pub struct Interpreter {
    i: u16,                      // Index register
    pc: usize,                   // Program counter
    stack: Vec<u16>,             // Stack
    memory: Memory,              // Memory
    display: Option<Display>,    // Display
    timers: Arc<RwLock<Timers>>, // Timers
    registers: RegisterArray,    // Variable registers (V0..=VF)
    ips: u64,                    // Instructions per second
}

impl Interpreter {
    const MEMORY_SIZE: usize = 4096;
    /// The start location for program-accessible memory.
    const MEMORY_OFFSET: usize = 0x200;
    const REGISTER_COUNT: usize = 16;

    /// Creates a new CHIP-8 instance with all fields zero-initialized.
    /// To attach a display to the interpreter, use
    /// [`attach_display`](Self::attach_display).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Attaches the display to the interpreter.
    pub fn attach_display(&mut self, display: Display) {
        self.display = Some(display);
        info!("Attached display [success: true]");
    }

    /// Sets the number of instructions to execute per second.
    pub fn with_ips(&mut self, ips: u64) {
        self.ips = ips;
    }

    /// Creates a new thread for the fetch/decode/execute loop.
    fn main(intr: Arc<RwLock<Interpreter>>, rx: Receiver<VirtualKeyCode>) {
        thread::spawn(move || {
            std::panic::set_hook(Box::new(|info| {
                error!("{}", info);
                std::process::exit(1);
            }));
            intr.write().unwrap().execute(&rx);
        });
    }

    /// Creates a new thread for the 60Hz timer loop.
    fn timers(intr: &Arc<RwLock<Interpreter>>) {
        let timers = intr.read().unwrap().get_timers();
        thread::spawn(move || loop {
            timers.write().unwrap().update();
            std::thread::sleep(std::time::Duration::from_millis(1000 / 60));
        });
    }

    /// Starts the window event loop.
    fn ui(el: EventLoop<()>, tx: Sender<VirtualKeyCode>) {
        let mut input = WinitInputHelper::new();
        el.run(move |event, _, cf| {
            *cf = ControlFlow::Poll;

            if input.update(&event) {
                if input.quit() {
                    *cf = ControlFlow::Exit;
                    return;
                }

                let key = input::KEYMAP.keys().find(|&&key| input.key_pressed(key));
                if let Some(&key) = key {
                    tx.send(key).unwrap();
                }
            }
        });
    }

    /// Loads the rom into the CHIP-8 interpreter's memory buffer.
    pub fn load_rom(&mut self, rom: &[u8]) {
        self.i = 0;
        self.pc = Self::MEMORY_OFFSET;
        self.stack = Vec::new();
        self.memory = Memory::default();
        self.timers = Arc::new(RwLock::new(Timers::default()));
        self.registers = RegisterArray::default();

        self.memory[font::MEMORY_RANGE].copy_from_slice(font::FONT);
        self.memory[Self::MEMORY_OFFSET..Self::MEMORY_OFFSET + rom.len()].copy_from_slice(rom);
        info!("Loaded ROM [size: {}]", rom.len());
    }

    /// Obtains a reference to the timers.
    fn get_timers(&self) -> Arc<RwLock<Timers>> {
        Arc::clone(&self.timers)
    }

    /// Obtains a mutable reference to the attached display.
    fn get_display_mut(&mut self) -> &mut Display {
        if let Some(display) = self.display.as_mut() {
            display
        } else {
            error!("No display attached");
            std::process::exit(1)
        }
    }

    /// Fetches the instruction at the PC (program counter) from memory.
    fn fetch(&mut self) -> u16 {
        let inst = u16::from_be_bytes([self.memory[self.pc], self.memory[self.pc + 1]]);
        self.pc += 2;
        inst
    }

    /// Decodes the instruction fetched with [`fetch`](Self::fetch).
    fn decode(&mut self) -> Instruction {
        Instruction::from(self.fetch())
    }

    /// Executes the current instruction, pausing for ~1.4ms to
    /// achieve a speed of approximately 700 instructions/second.
    fn execute(&mut self, rx: &Receiver<VirtualKeyCode>) {
        loop {
            let inst = self.decode();
            debug!("Processing instruction [{:?}]", inst);
            trace!(
                "Timers: [sound: {}] [delay: {}]",
                self.timers.read().unwrap().sound,
                self.timers.read().unwrap().delay
            );
            trace!("Registers: {:?}", self.registers);
            match inst.nibbles[..] {
                [0, 0, 0xE, 0] => self.get_display_mut().clear(), // 00E0
                [1, n1, n2, n3] => self.jump(n1, n2, n3),         // 1NNN
                [0, 0, 0xE, 0xE] => self.subroutine_return(),     // 00EE
                [2, n1, n2, n3] => self.call_subroutine(n1, n2, n3), // 2NNN
                [3, register, n1, n2] => self.skip_vx(usize::from(register), n1, n2, true), // 3XNN
                [4, register, n1, n2] => self.skip_vx(usize::from(register), n1, n2, false), // 4XNN
                [5, vx, vy, 0] => self.skip_vxy(usize::from(vx), usize::from(vy), true), // 5XY0
                [9, vx, vy, 0] => self.skip_vxy(usize::from(vx), usize::from(vy), false), // 9XY0
                [6, register, n1, n2] => self.set_register(usize::from(register), n1, n2), // 6XNN
                [7, register, n1, n2] => self.add_to_register(usize::from(register), n1, n2), // 7XNN
                [8, x, y, 0] => self.set(usize::from(x), usize::from(y)), // 8XY0
                [8, x, y, 1] => self.or(usize::from(x), usize::from(y)),  // 8XY1
                [8, x, y, 2] => self.and(usize::from(x), usize::from(y)), // 8XY2
                [8, x, y, 3] => self.xor(usize::from(x), usize::from(y)), // 8XY3
                [8, x, y, 4] => self.add(usize::from(x), usize::from(y)), // 8XY4
                [8, x, y, 5] => self.sub(usize::from(x), usize::from(x), usize::from(y)), // 8XY5
                [8, x, y, 7] => self.sub(usize::from(x), usize::from(y), usize::from(x)), // 8XY7
                [8, x, _, 6] => self.shift_right(usize::from(x)),         // 8XY6
                [8, x, _, 0xE] => self.shift_left(usize::from(x)),        // 8XYE
                [0xA, n1, n2, n3] => self.set_memory_ptr(n1, n2, n3),     // ANNN
                [0xB, n1, n2, n3] => self.jump_with_offset(n1, n2, n3),   // BNNN
                [0xC, x, n1, n2] => self.random(usize::from(x), n1, n2),  // CXNN
                [0xD, vx, vy, height] => self.draw_sprite(usize::from(vx), usize::from(vy), height), // DXYN
                [0xE, vx, 0x9, 0xE] => self.skip_key(usize::from(vx), rx, true), // EX9E
                [0xE, vx, 0xA, 0x1] => self.skip_key(usize::from(vx), rx, false), // EXA1
                [0xF, x, 0, 7] => self.timer_to_vx(usize::from(x)),              // FX07
                [0xF, x, 1, 5] => self.vx_to_timer(usize::from(x), true),        // FX15
                [0xF, x, 1, 8] => self.vx_to_timer(usize::from(x), false),       // FX18
                [0xF, x, 0x1, 0xE] => self.add_to_index(usize::from(x)),         // FX1E
                [0xF, vx, 0x0, 0xA] => self.get_key(usize::from(vx), rx),        // FX0A
                [0xF, vx, 2, 9] => self.font_character(usize::from(vx)),         // FX29
                [0xF, vx, 3, 3] => self.conversion(usize::from(vx)),             // FX33
                [0xF, vx, 5, 5] => self.store_to_memory(usize::from(vx)),        // FX55
                [0xF, vx, 6, 5] => self.load_from_memory(usize::from(vx)),       // FX65
                _ => {
                    error!("Unknown opcode: {:?}", &inst);
                    std::process::exit(1);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(1000 / self.ips));
        }
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#00ee-and-2nnn-subroutines>
    fn call_subroutine(&mut self, n1: u8, n2: u8, n3: u8) {
        self.stack.push(u16::try_from(self.pc).unwrap());
        let pc = usize::from_be_bytes([0, 0, 0, 0, 0, 0, n1, bits::recombine(n2, n3)]);
        self.pc = pc;
        trace!("call_subroutine: set PC to {pc}");
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#00ee-and-2nnn-subroutines>
    fn subroutine_return(&mut self) {
        let pc = usize::from(self.stack.pop().unwrap());
        self.pc = pc;
        trace!("subroutine_return: set PC to {pc}");
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#3xnn-4xnn-5xy0-and-9xy0-skip>
    fn skip_vx(&mut self, register: usize, n1: u8, n2: u8, equality: bool) {
        let vx = self.registers[register];
        let x = bits::recombine(n1, n2);
        if (equality && vx == x) || (!equality && vx != x) {
            trace!("skip_vx: incremented pc by 2");
            self.pc += 2;
        }
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#3xnn-4xnn-5xy0-and-9xy0-skip>
    fn skip_vxy(&mut self, vx: usize, vy: usize, equality: bool) {
        let vx = self.registers[vx];
        let vy = self.registers[vy];
        if (equality && vx == vy) || (!equality && vx != vy) {
            trace!("skip_vxy: incremented pc by 2");
            self.pc += 2;
        }
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#8xy0-set>
    fn set(&mut self, vx: usize, vy: usize) {
        self.registers[vx] = self.registers[vy];
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#8xy1-binary-or>
    fn or(&mut self, vx: usize, vy: usize) {
        self.registers[vx] |= self.registers[vy];
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#8xy2-binary-and>
    fn and(&mut self, vx: usize, vy: usize) {
        self.registers[vx] &= self.registers[vy];
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#8xy3-logical-xor>
    fn xor(&mut self, vx: usize, vy: usize) {
        self.registers[vx] ^= self.registers[vy];
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#8xy4-add>
    fn add(&mut self, vx: usize, vy: usize) {
        let x = usize::from(self.registers[vx]);
        let y = usize::from(self.registers[vy]);
        self.registers[vx] = self.registers[vx].wrapping_add(self.registers[vy]);
        self.registers[0xF] = u8::from(x + y > 255);
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#8xy5-and-8xy7-subtract>
    fn sub(&mut self, vx: usize, lhs: usize, rhs: usize) {
        let lhs = self.registers[lhs];
        let rhs = self.registers[rhs];
        self.registers[0xF] = u8::from(lhs > rhs);
        self.registers[vx] = lhs.wrapping_sub(rhs);
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#8xy6-and-8xye-shift>
    fn shift_left(&mut self, vx: usize) {
        let shifted = bits::set(7, self.registers[vx]);
        self.registers[vx] <<= 1;
        self.registers[0xF] = u8::from(shifted);
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#8xy6-and-8xye-shift>
    fn shift_right(&mut self, vx: usize) {
        let shifted = bits::set(0, self.registers[vx]);
        self.registers[vx] >>= 1;
        self.registers[0xF] = u8::from(shifted);
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#cxnn-random>
    fn random(&mut self, vx: usize, n1: u8, n2: u8) {
        let address = bits::recombine(n1, n2);
        let r: u8 = rand::thread_rng().gen();
        self.registers[vx] = address & r;
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#fx07-fx15-and-fx18-timers>
    fn timer_to_vx(&mut self, vx: usize) {
        let timers = self.get_timers();
        let timers = timers.read().unwrap();
        self.registers[vx] = timers.delay;
        trace!(
            "timer_to_vx: written value {} to register V{vx:01X}",
            timers.delay
        );
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#fx07-fx15-and-fx18-timers>
    fn vx_to_timer(&mut self, vx: usize, delay: bool) {
        let timers = self.get_timers();
        let value = self.registers[vx];
        let mut timers = timers.write().unwrap();
        let timer = if delay {
            &mut timers.delay
        } else {
            &mut timers.sound
        };
        *timer = value;
        trace!("vx_to_timer: set timer [delay: {}] to {}", delay, value);
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#fx1e-add-to-index>
    fn add_to_index(&mut self, vx: usize) {
        self.i += u16::from(self.registers[vx]);
        if self.i > 0x1000 {
            self.registers[0xF] = 1;
        }
        trace!(
            "add_to_index: added {} to index register",
            self.registers[vx]
        );
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#1nnn-jump>
    fn jump(&mut self, n1: u8, n2: u8, n3: u8) {
        let pc = usize::from_be_bytes([0, 0, 0, 0, 0, 0, n1, bits::recombine(n2, n3)]);
        self.pc = pc;
        trace!("jump: set PC to {pc}");
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#bnnn-jump-with-offset>
    fn jump_with_offset(&mut self, n1: u8, n2: u8, n3: u8) {
        let address = u16::from_be_bytes([n1, bits::recombine(n2, n3)]);
        let pc = usize::from(address) + usize::from(self.registers[0x0]);
        self.pc = pc;
        trace!("jump_with_offset: set PC to {pc}");
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#fx29-font-character>
    fn font_character(&mut self, vx: usize) {
        let c = self.registers[vx];
        trace!("font [char: {:#X}]", c);
        let start = u16::try_from(*font::MEMORY_RANGE.start()).unwrap();
        self.i = start + u16::from(c * 5);
        trace!("font [i: {:#X}]", self.i);
        trace!("font_character: set I to {}", self.i);
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#fx33-binary-coded-decimal-conversion>
    fn conversion(&mut self, vx: usize) {
        let x = usize::from(self.registers[vx]);
        let i = usize::from(self.i);
        let left = u8::try_from(digit(2, x)).unwrap();
        let mid = u8::try_from(digit(1, x)).unwrap();
        let right = u8::try_from(digit(0, x)).unwrap();
        self.memory[i..i + 3].copy_from_slice(&[left, mid, right]);
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#fx55-and-fx65-store-and-load-memory>
    fn store_to_memory(&mut self, vx: usize) {
        let len = (0x0..=vx).count();
        let i = usize::from(self.i);
        self.memory[i..i + len].copy_from_slice(&self.registers[0x0..=vx]);
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#fx55-and-fx65-store-and-load-memory>
    fn load_from_memory(&mut self, vx: usize) {
        let len = (0x0..=vx).count();
        let i = usize::from(self.i);
        self.registers[0x0..=vx].copy_from_slice(&self.memory[i..i + len]);
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#6xnn-set>
    fn set_register(&mut self, register: usize, n1: u8, n2: u8) {
        let value = bits::recombine(n1, n2);
        self.registers[register] = value;
        trace!("set_register: V{register:01X} => {value}");
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#7xnn-add>
    fn add_to_register(&mut self, register: usize, n1: u8, n2: u8) {
        let value = bits::recombine(n1, n2);
        self.registers[register] = self.registers[register].wrapping_add(value);
        trace!("add_to_register: V{register:01X} + {value}");
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#annn-set-index>
    fn set_memory_ptr(&mut self, n1: u8, n2: u8, n3: u8) {
        let value = u16::from_be_bytes([n1, bits::recombine(n2, n3)]);
        self.i = value;
        trace!("set_memory_ptr: set index register I to {value}");
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#dxyn-display>
    fn draw_sprite(&mut self, vx: usize, vy: usize, height: u8) {
        let x = self.registers[vx] % Display::WIDTH;
        let y = self.registers[vy] % Display::HEIGHT;
        trace!("x: {x} y: {y} height: {height}");
        self.registers[0xF] = 0;
        for (idx, y) in (y..y + height).enumerate() {
            let sprite = self.memory[usize::from(self.i)..][idx];
            for (n, x) in (x..x + 8).enumerate() {
                let n = u8::try_from(n).unwrap();
                let on = bits::set(7 - n, sprite);
                if on && self.get_display_mut().flip(x, y, [0xFF, 0xFF, 0xFF, 0xFF]) {
                    self.registers[0xF] = 1;
                }
                if x >= Display::WIDTH - 1 {
                    break;
                }
            }
            if y >= Display::HEIGHT - 1 {
                break;
            }
        }
        self.get_display_mut().render();
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#ex9e-and-exa1-skip-if-key>
    fn get_key(&mut self, vx: usize, rx: &Receiver<VirtualKeyCode>) {
        'wait: loop {
            match rx.try_recv() {
                Ok(key) => {
                    let &key = input::KEYMAP.get(&key).unwrap();
                    self.registers[vx] = key;
                    trace!("Stored key {key:01X} in register V{vx:01X}");
                    break 'wait;
                }
                Err(e) => match e {
                    TryRecvError::Empty => {}
                    TryRecvError::Disconnected => {
                        error!("Key receiver hung up");
                        std::process::exit(1);
                    }
                },
            }
        }
    }

    /// <https://tobiasvl.github.io/blog/write-a-chip-8-emulator/#ex9e-and-exa1-skip-if-key>
    fn skip_key(&mut self, vx: usize, rx: &Receiver<VirtualKeyCode>, press: bool) {
        if let Ok(key) = rx.recv_timeout(std::time::Duration::from_millis(100)) {
            let &key = input::KEYMAP.get(&key).unwrap();
            trace!("Key received: {key:01X} | VX: {}", self.registers[vx]);
            if press && self.registers[vx] == key {
                self.pc += 2;
                trace!("Incremented PC by 2");
            } else if !press && self.registers[vx] != key {
                self.pc += 2;
                trace!("Incremented PC by 2");
            }
        }
    }
}

/// The CHIP-8 display.
pub struct Display {
    /// The pixels which are copied into [`pixels`](Self::pixels)
    /// upon a call to [`render`](Self::render).
    scratch_pixels: [u8; Self::WIDTH as usize * Self::HEIGHT as usize * 4],
    /// Keeps the window alive.
    _window: Window,
    /// A pixel buffer of the pixels currently being displayed.
    pixels: Pixels,
}

impl Display {
    const WIDTH: u8 = 64;
    const HEIGHT: u8 = 32;

    /// Creates a new Window and pixel buffer attached to the given [`EventLoop`](winit::event_loop::EventLoop).
    ///
    /// # Panics
    /// This function will panic if the window fails to be created.
    #[must_use]
    pub fn new(el: &EventLoop<()>) -> Self {
        let window = {
            let size = LogicalSize::new(u32::from(Self::WIDTH), u32::from(Self::HEIGHT));
            let scaled = LogicalSize::new(
                f64::from(Self::WIDTH) * 10.0,
                f64::from(Self::HEIGHT) * 10.0,
            );
            WindowBuilder::new()
                .with_title("CHIP-8")
                .with_resizable(false)
                .with_inner_size(scaled)
                .with_min_inner_size(size)
                .build(el)
                .unwrap()
        };

        let pixels = {
            let size = window.inner_size();
            let texture = SurfaceTexture::new(size.width, size.height, &window);
            Pixels::new(u32::from(Self::WIDTH), u32::from(Self::HEIGHT), texture).unwrap()
        };

        Self {
            scratch_pixels: [0; Self::WIDTH as usize * Self::HEIGHT as usize * 4],
            _window: window,
            pixels,
        }
    }

    /// Clears the display.
    fn clear(&mut self) {
        self.scratch_pixels = [0; Self::WIDTH as usize * Self::HEIGHT as usize * 4];
        self.render();
    }

    /// Renders the [`scratch_pixels`](Self::scratch_pixels) to the screen, overwriting the existing [`pixels`](Self::pixels).
    fn render(&mut self) {
        self.draw();
        self.pixels.render().unwrap();
        trace!("{:?}", self);
    }

    /// Draws the [`scratch_pixels`](Self::scratch_pixels) to the live pixel buffer.
    fn draw(&mut self) {
        let frame = self.pixels.get_frame_mut();
        for (pixel, scratch_pixel) in frame
            .chunks_exact_mut(4)
            .zip(self.scratch_pixels.chunks_exact(4))
        {
            pixel.copy_from_slice(scratch_pixel);
        }
    }

    /// Flips the pixel at (`x`, `y`) with the RGBA values specified by `rgba`.
    fn flip(&mut self, x: u8, y: u8, rgba: [u8; 4]) -> bool {
        let x = usize::from(x);
        let y = usize::from(y);
        let idx = (y * usize::from(Self::WIDTH) + x) * 4;
        let cur = &self.scratch_pixels[idx..idx + 4];
        let pixels = if cur == [0xFF, 0xFF, 0xFF, 0xFF] {
            [0x0, 0x0, 0x0, 0x0]
        } else {
            rgba
        };
        self.scratch_pixels[idx..idx + 4].copy_from_slice(&pixels);
        self.scratch_pixels[idx..idx + 4] == [0x0, 0x0, 0x0, 0x0]
    }

    /// Gets the state of the pixel at (`x`, `y`).
    fn get_at(&self, x: u8, y: u8) -> u8 {
        let x = usize::from(x);
        let y = usize::from(y);
        let idx = (y * usize::from(Self::WIDTH) + x) * 4;
        self.scratch_pixels[idx]
    }
}

impl fmt::Debug for Display {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = String::new();
        for y in 0..Display::HEIGHT {
            for x in 0..Display::WIDTH {
                s += if self.get_at(x, y) == 0x0 { " " } else { "█" };
            }
            s += "\n";
        }
        write!(f, "{s}")
    }
}

/// The CHIP-8 delay and sound timers.
#[derive(Debug, Default)]
struct Timers {
    delay: u8,
    sound: u8,
}

impl Timers {
    /// Updates the timers, decrementing both by one if
    /// greater than 0. Plays a sound as long as the sound
    /// timer greater than 0.
    fn update(&mut self) {
        if self.delay > 0 {
            self.delay -= 1;
        }
        if self.sound > 0 {
            self.sound -= 1;
            // TODO: play sound
        }
        trace!(
            "Updated timers: [sound: {}] [delay: {}]",
            self.sound,
            self.delay
        );
    }
}

wrapper! {
    /// The CHIP-8 memory buffer.
    Memory => Interpreter::MEMORY_SIZE,
    /// The CHIP-8 registers.
    RegisterArray => Interpreter::REGISTER_COUNT
}

/// A CHIP-8 instruction.
#[derive(PartialEq)]
pub struct Instruction {
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
        for nibble in &self.nibbles {
            write!(f, "{nibble:X}")?;
        }
        Ok(())
    }
}

/// Helper functions for bit operations.
mod bits {
    /// Returns a bool indicating whether the bit at index n is set.
    /// Bits are indexed from the least-significant bit to the
    /// most-significant bit.
    pub const fn set(n: u8, bits: u8) -> bool {
        (bits & (1 << n)) != 0
    }

    /// A helper utility for reconstructing a single 8-bit integer
    /// from two 4-bit nibbles.
    pub const fn recombine(upper: u8, lower: u8) -> u8 {
        (upper << 4) | lower
    }
}

/// Returns the digit at index `i` in the number `n`. Numbers are indexed from
/// least-significant to most-significant.
fn digit(i: u32, n: usize) -> usize {
    (n / (10usize.pow(i))) % 10
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instruction() {
        let val = 0b0010_1110; // 46
        let inst = Instruction::from(val);
        assert_eq!(
            inst,
            Instruction {
                nibbles: vec![0, 0, 0b0010, 0b1110]
            }
        );
    }

    #[test]
    fn to_digits() {
        let n = 456;
        assert_eq!(digit(0, n), 6);
        assert_eq!(digit(1, n), 5);
        assert_eq!(digit(2, n), 4);
    }
}
