use esp_idf_sys as _;

use esp_idf_hal::peripherals::Peripherals;

pub mod paper;
use paper::{Paper, PaperPeripherals};

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

    let mut framebuffer = Framebuffer::new();
    framebuffer.text(fb::Paint::Darken, 150, 150, 80., "Hello::<World>");
    let mut p = paper.powered_on();
    p.clear();
    p.draw_framebuffer(&framebuffer);
}
