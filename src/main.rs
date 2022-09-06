use std::ffi::{c_void, CString};

use chrono::NaiveDateTime;
use embedded_hal::blocking::delay::DelayMs;
use esp_idf_hal::{cpu::Core, delay::Ets, peripherals::Peripherals};
use esp_idf_sys::{
    self as _, esp_partition_erase_range, esp_partition_find_first, esp_partition_read,
    esp_partition_subtype_t_ESP_PARTITION_SUBTYPE_ANY, esp_partition_t,
    esp_partition_type_t_ESP_PARTITION_TYPE_ANY, esp_partition_write, EspError,
};

pub mod paper;
use paper::{DrawMode, Paper, PaperPeripherals, PreparedFramebuffer};

pub mod fb;
use fb::{Framebuffer, Paint};

pub mod thread;

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

    let draw_worker = thread::spawn(Core::Core1, || {
        let counter = find_counter_partition();
        let value = read_and_increment_counter(&counter);

        let timestamp: i64 = value.into();
        let mut time = NaiveDateTime::from_timestamp(60 * timestamp, 0);

        // Advance time up to a point given at compile time. This will only call
        // `reset_and_write_counter` once after flashing, and normal operation will resume
        // afterwards. Cannot be used to decrease the stored timestamp.
        if let Some(set_time) = option_env!("PAPERSLAVE_ADVANCE_TIME") {
            let set_time = NaiveDateTime::parse_from_str(set_time, "%Y-%m-%d %H:%M").unwrap();
            if set_time > time {
                time = set_time;
                let value = set_time.timestamp() / 60;
                set_counter(&counter, value.try_into().unwrap());
            }
        }

        let time_string = time.format("%H:%M").to_string();
        let date_string = time.format("%-d.%-m.%Y").to_string();

        let mut framebuffer = Framebuffer::new();
        framebuffer.text_centered(Paint::Darken, fb::WIDTH / 2, 96, 90., "Aikamme");
        framebuffer.text_centered(Paint::Darken, fb::WIDTH / 2, 405, 454., &time_string);
        framebuffer.text_centered(Paint::Darken, fb::WIDTH / 2, 500, 90., &date_string);
        PreparedFramebuffer::prepare(&framebuffer, DrawMode::DirectUpdateBinary)
    });

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

/// Monotonically increasing counter implementation which minimizes the use of the expensive flash
/// erase operation.
fn read_and_increment_counter(partition: &esp_partition_t) -> u32 {
    let size = partition.size;

    let mut buffer = vec![0_u8; size as usize];
    partition_read(partition, 0, &mut buffer).unwrap();

    /* First 4 bytes of the counter are a base value. */
    let base = u32::from_be_bytes(buffer[0..4].try_into().unwrap());

    /* The rest is an offset that will be added to the base value. The offset is implemented as a
     * unary counter where every increment flips one bit from 1 to 0. When all the bits are fully
     * flipped, the offset region is reset and the base value is updated. */
    let offset_region = &buffer[4..];
    let unary_head_index = offset_region.partition_point(|&x| x == 0);
    let unary_bits_head_byte;

    match offset_region.get(unary_head_index) {
        /* Increase the offset unary counter by one bit. */
        Some(&head_byte) => {
            unary_bits_head_byte = head_byte.leading_zeros();

            let new_head_byte = head_byte >> 1;
            let byte_index = 4 + unary_head_index as u32;
            partition_write(partition, byte_index, &new_head_byte.to_be_bytes()).unwrap();
        }
        /* The offset unary counter is already full, reset it and update base. */
        None => {
            unary_bits_head_byte = 0;

            let new_base = base + (size - 4) * 8 + 1;
            set_counter(partition, new_base);
        }
    }

    let offset = unary_head_index as u32 * 8 + unary_bits_head_byte;
    return base + offset;
}

/// Overwrite counter in a way that **does not** conserve flash erase cycles.
///
/// Should not be used repeatedly.
fn set_counter(partition: &esp_partition_t, value: u32) {
    partition_erase(partition, 0, partition.size).unwrap();
    partition_write(partition, 0, &value.to_be_bytes()).unwrap();
}

fn partition_erase(partition: &esp_partition_t, offset: u32, size: u32) -> Result<(), EspError> {
    unsafe {
        return EspError::convert(esp_partition_erase_range(partition, offset, size));
    }
}

fn partition_write(partition: &esp_partition_t, offset: u32, data: &[u8]) -> Result<(), EspError> {
    unsafe {
        return EspError::convert(esp_partition_write(
            partition,
            offset,
            data.as_ptr() as *const c_void,
            data.len() as u32,
        ));
    }
}

fn partition_read(
    partition: &esp_partition_t,
    offset: u32,
    buffer: &mut [u8],
) -> Result<(), EspError> {
    unsafe {
        return EspError::convert(esp_partition_read(
            partition,
            offset,
            buffer.as_mut_ptr() as *mut c_void,
            buffer.len() as u32,
        ));
    }
}
