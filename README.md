# etherea

An emulator(/interpreter) for CHIP-8, the 1970s programming language. 

All ROMs in the `roms/` directory of this repository have been tested and should work. Other ROMs will be added when I resolve bugs.

## Install

With `cargo`:

```sh
cargo install etherea
```

## Usage

**Run a ROM:**
 
```sh
etherea run path/to/rom.ch8
```

**Disassemble a ROM:**

```sh
etherea disassemble path/to/rom.ch8
```

**View options:**

```sh
etherea --help
```

## References

- [https://en.wikipedia.org/wiki/CHIP-8](https://en.wikipedia.org/wiki/CHIP-8)
- [https://tobiasvl.github.io/blog/write-a-chip-8-emulator](https://tobiasvl.github.io/blog/write-a-chip-8-emulator)
