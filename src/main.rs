use std::{
    ffi::{c_void, CString},
    fmt::Debug,
};

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use embedded_hal::{blocking::delay::DelayMs, digital::v2::InputPin};
use esp_idf_hal::{cpu::Core, delay::FreeRtos, peripherals::Peripherals};
use esp_idf_sys::{
    self as _, esp_partition_erase_range, esp_partition_find_first, esp_partition_read,
    esp_partition_subtype_t_ESP_PARTITION_SUBTYPE_ANY, esp_partition_t,
    esp_partition_type_t_ESP_PARTITION_TYPE_ANY, esp_partition_write, EspError,
};

pub mod paper;
use paper::{DrawMode, Paper, PaperPeripherals, PreparedFramebuffer};

pub mod fb;
use fb::{Framebuffer, Paint, Rect};

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
        FreeRtos.delay_ms(2000_u32);
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

    {
        let mut p = paper.powered_on();
        p.quick_clear();
        let prepared = draw_worker.join().unwrap();
        p.draw(&prepared);
    }

    FreeRtos.delay_ms(3000_u32);

    {
        println!("entering adjust mode");
        #[derive(Clone, PartialEq)]
        struct State {
            field: AdjustField,
            time: NaiveDateTime,
        }

        let state = std::sync::Arc::new(std::sync::Mutex::new(State {
            field: AdjustField::Minutes,
            time: NaiveDateTime::from_timestamp(0, 0),
        }));

        let worker_state = state.clone();
        thread::spawn(Core::Core1, move || {
            paper.powered_on().clear();
            let mut framebuffer = Framebuffer::new();
            let mut prev_framebuffer = Framebuffer::new();
            let mut local_state = worker_state.lock().unwrap().clone();
            let mut dirty = false;
            'redraw: loop {
                framebuffer.clear();
                const BUTTONS_START: i32 = 240;
                const BUTTONS_SPACE: i32 = 69;
                for (i, text) in [Some("RST"), None, Some("NEXT"), Some("-"), Some("+")]
                    .into_iter()
                    .enumerate()
                {
                    if let Some(text) = text {
                        let pos = BUTTONS_START + i as i32 * BUTTONS_SPACE;
                        framebuffer.rect(
                            Paint::Darken,
                            Rect {
                                x: pos - 20,
                                y: 0,
                                w: 40,
                                h: 4,
                            },
                        );
                        framebuffer.text_centered(Paint::Darken, pos, 30, 30., text);
                    }
                }
                for (part, x, y) in [
                    (AdjustField::Hours, 380, 200),
                    (AdjustField::Minutes, 540, 200),
                    (AdjustField::Days, 270, 320),
                    (AdjustField::Months, 430, 320),
                    (AdjustField::Years, 630, 320),
                    (AdjustField::Store, 460, 450),
                ] {
                    framebuffer.text_centered(
                        Paint::Darken,
                        x,
                        y,
                        94.,
                        &part.format(local_state.time),
                    );
                    if part == local_state.field {
                        framebuffer.rect(
                            Paint::Darken,
                            Rect {
                                x: x - 40,
                                y: y + 10,
                                w: 80,
                                h: 6,
                            },
                        );
                    }
                }
                let prepared = PreparedFramebuffer::prepare_difference(
                    &prev_framebuffer,
                    &framebuffer,
                    DrawMode::DirectUpdateBinary,
                );
                paper.powered_on().draw(&prepared);
                std::mem::swap(&mut prev_framebuffer, &mut framebuffer);
                let mut tries = 0;
                loop {
                    {
                        let updated_state = worker_state.lock().unwrap();
                        if &*updated_state != &local_state {
                            local_state = updated_state.clone();
                            dirty = true;
                            continue 'redraw;
                        }
                    }
                    if dirty {
                        tries += 1;
                        if tries >= 500 {
                            paper.powered_on().quick_clear();
                            prev_framebuffer.clear();
                            dirty = false;
                            continue 'redraw;
                        }
                    }
                    FreeRtos.delay_ms(10u32);
                }
            }
        });

        let nxt_button = pins.gpio35.into_input().unwrap();
        let backward_button = pins.gpio34.into_input().unwrap();
        let forward_button = pins.gpio39.into_input().unwrap();

        fn check_press_latch<P: InputPin>(pin: &P) -> bool
        where
            P::Error: Debug,
        {
            if pin.is_low().unwrap() {
                while pin.is_low().unwrap() {
                    FreeRtos.delay_ms(1u32);
                }
                true
            } else {
                false
            }
        }

        loop {
            if check_press_latch(&nxt_button) {
                println!("nxt");
                let mut state = state.lock().unwrap();
                state.field = state.field.cycle();
            }
            if check_press_latch(&backward_button) {
                println!("back");
                let mut state = state.lock().unwrap();
                state.time = adjust(state.field, AdjustDirection::Backward, state.time);
            }
            if check_press_latch(&forward_button) {
                println!("forward");
                let mut state = state.lock().unwrap();
                if matches!(state.field, AdjustField::Store) {
                    // TODO
                } else {
                    state.time = adjust(state.field, AdjustDirection::Forward, state.time);
                }
            }
            FreeRtos.delay_ms(1u32);
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum AdjustField {
    Minutes,
    Hours,
    Days,
    Months,
    Years,
    Store,
}

#[derive(Clone, Copy)]
enum AdjustDirection {
    Forward,
    Backward,
}

impl AdjustField {
    fn cycle(self) -> Self {
        match self {
            AdjustField::Minutes => AdjustField::Hours,
            AdjustField::Hours => AdjustField::Days,
            AdjustField::Days => AdjustField::Months,
            AdjustField::Months => AdjustField::Years,
            AdjustField::Years => AdjustField::Store,
            AdjustField::Store => AdjustField::Minutes,
        }
    }

    fn format(self, datetime: NaiveDateTime) -> String {
        let format_string = match self {
            AdjustField::Minutes => "%M",
            AdjustField::Hours => "%H",
            AdjustField::Days => "%d",
            AdjustField::Months => "%m",
            AdjustField::Years => "%Y",
            AdjustField::Store => "OK",
        };
        datetime.format(format_string).to_string()
    }
}

fn adjust(
    field: AdjustField,
    direction: AdjustDirection,
    datetime: NaiveDateTime,
) -> NaiveDateTime {
    let mut date = datetime.date();
    let mut time = datetime.time();
    let mut overflow_days = 0;

    let adjust_time = match direction {
        AdjustDirection::Forward => NaiveTime::overflowing_add_signed,
        AdjustDirection::Backward => NaiveTime::overflowing_sub_signed,
    };
    let adjust_date_duration = |date: NaiveDate, duration| {
        match direction {
            AdjustDirection::Forward => date.checked_add_signed(duration),
            AdjustDirection::Backward => date.checked_sub_signed(duration),
        }
        .unwrap_or(date)
    };
    let adjust_date_months = |date: NaiveDate, months| {
        match direction {
            AdjustDirection::Forward => date.checked_add_months(months),
            AdjustDirection::Backward => date.checked_sub_months(months),
        }
        .unwrap_or(date)
    };

    match field {
        AdjustField::Minutes => {
            (time, overflow_days) = adjust_time(&time, chrono::Duration::minutes(1))
        }
        AdjustField::Hours => {
            (time, overflow_days) = adjust_time(&time, chrono::Duration::hours(1))
        }
        AdjustField::Days => date = adjust_date_duration(date, chrono::Duration::days(1)),
        AdjustField::Months => date = adjust_date_months(date, chrono::Months::new(1)),
        AdjustField::Years => date = adjust_date_months(date, chrono::Months::new(12)),
        AdjustField::Store => {}
    }
    date = adjust_date_duration(date, chrono::Duration::seconds(overflow_days));
    date.and_time(time)
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
