use std::thread;

use esp_idf_sys as _;

use embedded_hal::blocking::delay::DelayMs;
use esp_idf_hal::{delay::Ets, peripherals::Peripherals};

pub mod paper;
use paper::{DrawMode, Paper, PaperPeripherals, PreparedFramebuffer};

pub mod fb;
use fb::Framebuffer;

fn main() {
    let peripherals = Peripherals::take().unwrap();

    let pins = peripherals.pins;
    let rmt = peripherals.rmt;
    let mut paper = Paper::init(PaperPeripherals {
        gpio0: pins.gpio0,
        gpio2: pins.gpio2,
        gpio4: pins.gpio4,
        gpio5: pins.gpio5,
        gpio15: pins.gpio15,
        gpio18: pins.gpio18,
        gpio19: pins.gpio19,
        gpio21: pins.gpio21,
        gpio22: pins.gpio22,
        gpio23: pins.gpio23,
        gpio25: pins.gpio25,
        gpio26: pins.gpio26,
        gpio27: pins.gpio27,
        gpio32: pins.gpio32,
        gpio33: pins.gpio33,
        rmt_channel1: rmt.channel1,
    });

    // Avoid leaving display in indeterminate state when using cargo-espflash.
    // Disabled in release mode because of the 2s time budget.
    #[cfg(debug_assertions)]
    {
        println!("waiting for debug delay");
        Ets.delay_ms(3000_u32);
        println!("wait over");
    }

    let draw_worker = thread::Builder::new()
        .stack_size(6 * 1024)
        .spawn(|| {
            let mut framebuffer = Framebuffer::new();
            framebuffer.text(fb::Paint::Darken, 10, 330, 430., "00:00");
            PreparedFramebuffer::prepare(&framebuffer, DrawMode::DirectUpdateBinary)
        })
        .unwrap();

    let mut p = paper.powered_on();
    p.quick_clear();
    let prepared = draw_worker.join().unwrap();
    p.draw(&prepared);
}
