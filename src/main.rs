use std::{
    ffi::{c_void, CString},
    thread,
};

use embedded_hal::blocking::delay::DelayMs;
use esp_idf_hal::{delay::Ets, peripherals::Peripherals};
use esp_idf_sys::{
    self as _, esp_partition_erase_range, esp_partition_find_first, esp_partition_read,
    esp_partition_subtype_t_ESP_PARTITION_SUBTYPE_ANY, esp_partition_t,
    esp_partition_type_t_ESP_PARTITION_TYPE_ANY, esp_partition_write,
};

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

fn find_counter_partition() -> esp_partition_t {
    let partition_name = CString::new("counter").unwrap();
    unsafe {
        return *esp_partition_find_first(
            esp_partition_type_t_ESP_PARTITION_TYPE_ANY,
            esp_partition_subtype_t_ESP_PARTITION_SUBTYPE_ANY,
            partition_name.as_ptr(),
        );
    }
}

fn read_and_increment_counter(partition: &esp_partition_t) -> u32 {
    let size = partition.size;

    let mut buffer = vec![0_u8; size as usize];
    unsafe {
        esp_partition_read(partition, 0, buffer.as_mut_ptr() as *mut c_void, size);
    }

    let base = u32::from_be_bytes(buffer[0..4].try_into().unwrap());

    let offset_region = &buffer[4..];
    let unary_head_index = offset_region.partition_point(|&x| x == 0);
    let unary_bits_head_byte;

    match offset_region.get(unary_head_index) {
        Some(&head_byte) => {
            unary_bits_head_byte = head_byte.leading_zeros();

            let new_head_byte = head_byte >> 1;
            let byte_index = 4 + unary_head_index as u32;
            unsafe {
                esp_partition_write(
                    partition,
                    byte_index,
                    new_head_byte.to_be_bytes().as_ptr() as *const c_void,
                    1,
                );
            }
        }
        None => {
            unary_bits_head_byte = 0;

            let new_base = base + (size - 4) * 8 + 1;
            unsafe {
                esp_partition_erase_range(partition, 0, size);
                esp_partition_write(
                    partition,
                    0,
                    new_base.to_be_bytes().as_ptr() as *const c_void,
                    4,
                );
            }
        }
    }

    let offset = unary_head_index as u32 * 8 + unary_bits_head_byte;
    return base + offset;
}
