Digital slave clock for Schneider Electric (aka ESMI, Westerstrand) minute pulse clock system.

### Required hardware
- LilyGo T5 4.7" (e-paper display + ESP32 microcontroller)
- Wall with Schneider Electric minute pulse clock system
    - The system has to provide power for at least 2 seconds every minute.
- Power adapter from clock 24V line to USB

### How to build (Linux)
- Install [Rust](https://www.rust-lang.org/tools/install).
- Install [Rust ESP32 toolchain](https://github.com/esp-rs/rust-build).
- Clone this repository, including submodules.
- Define LIBCLANG_PATH environment variable that points to your ESP32 libclang directory.
- Connect the microcontroller to your computer via USB.
- Run `sudo cargo espflash` to compile and write the program to the microcontroller.

### How to use
- Just plug the microcontroller to the wall with the adapter and it will work.
