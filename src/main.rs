use std::ffi::{c_void, CString};

use chrono::NaiveDateTime;
use embedded_hal::blocking::delay::DelayMs;
use esp_idf_hal::{cpu::Core, delay::FreeRtos, peripherals::Peripherals};
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

pub mod adjust;
use crate::adjust::{adjust_mode, AdjustButtons};

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
        FreeRtos.delay_ms(2000_u32);
        println!("wait over");
    }

    let draw_worker = thread::spawn(Core::Core1, || {
        let counter = find_counter_partition();
        let value = read_and_increment_counter(&counter);
        let time = datetime_from_counter(value);

        let time_string = time.format("%H:%M").to_string();
        let date_string = time.format("%-d.%-m.%Y").to_string();

        let mut framebuffer = Framebuffer::new();
        framebuffer.text_centered(Paint::Darken, fb::WIDTH / 2, 96, 90., "Aikamme");
        framebuffer.text_centered(Paint::Darken, fb::WIDTH / 2, 405, 454., &time_string);
        framebuffer.text_centered(Paint::Darken, fb::WIDTH / 2, 500, 90., &date_string);
        PreparedFramebuffer::prepare(&framebuffer, DrawMode::DirectUpdateBinary)
    });

    {
        let mut p = paper.powered_on();
        p.quick_clear();
        let prepared = draw_worker.join().unwrap();
        p.draw(&prepared);
    }

    FreeRtos.delay_ms(3000_u32);

    #[cfg(debug_assertions)]
    println!("entering adjust mode");

    adjust_mode(
        paper,
        AdjustButtons {
            field_cycle: pins.gpio35.into_input().unwrap().degrade(),
            backward: pins.gpio34.into_input().unwrap().degrade(),
            forward: pins.gpio39.into_input().unwrap().degrade(),
        },
    );
}

fn datetime_from_counter(counter: u32) -> NaiveDateTime {
    let minutes: i64 = counter.into();
    NaiveDateTime::from_timestamp(60 * minutes, 0)
}

fn counter_from_datetime(datetime: NaiveDateTime) -> u32 {
    let minutes = datetime.timestamp() / 60;
    if minutes >= 0 {
        minutes.try_into().unwrap_or(u32::MAX)
    } else {
        0
    }
}

fn clamp_datetime_to_counter(datetime: NaiveDateTime) -> NaiveDateTime {
    datetime_from_counter(counter_from_datetime(datetime))
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

fn read_counter(partition: &esp_partition_t) -> u32 {
    read_and_increment_counter_impl(partition, false)
}

fn read_and_increment_counter(partition: &esp_partition_t) -> u32 {
    read_and_increment_counter_impl(partition, true)
}

/// Monotonically increasing counter implementation which minimizes the use of the expensive flash
/// erase operation.
fn read_and_increment_counter_impl(partition: &esp_partition_t, increment: bool) -> u32 {
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

            if increment {
                let new_head_byte = head_byte >> 1;
                let byte_index = 4 + unary_head_index as u32;
                partition_write(partition, byte_index, &new_head_byte.to_be_bytes()).unwrap();
            }
        }
        /* The offset unary counter is already full, reset it and update base. */
        None => {
            unary_bits_head_byte = 0;

            if increment {
                let new_base = base + (size - 4) * 8 + 1;
                set_counter(partition, new_base);
            }
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
