use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use embedded_hal::{blocking::delay::DelayMs, digital::v2::InputPin};
use esp_idf_hal::{
    cpu::Core,
    delay::FreeRtos,
    gpio::{GpioPin, Input},
};

use crate::{
    fb::{Framebuffer, Paint, Rect},
    paper::{DrawMode, Paper, PreparedFramebuffer},
    thread, clamp_datetime_to_counter, read_counter, datetime_from_counter, find_counter_partition, counter_from_datetime, set_counter,
};

pub struct AdjustButtons {
    pub field_cycle: GpioPin<Input>,
    pub backward: GpioPin<Input>,
    pub forward: GpioPin<Input>,
}

#[derive(Clone, PartialEq)]
struct State {
    field: AdjustField,
    time: NaiveDateTime,
    changed: bool,
}

pub fn adjust_mode(mut paper: Paper, buttons: AdjustButtons) {
    let partition = find_counter_partition();
    let state = std::sync::Arc::new(std::sync::Mutex::new(State {
        field: AdjustField::Years,
        time: datetime_from_counter(read_counter(&partition)),
        changed: false,
    }));

    // Draw thread
    let worker_state = state.clone();
    thread::spawn(Core::Core1, move || {
        paper.powered_on().clear();
        let mut framebuffer = Framebuffer::new();
        let mut prev_framebuffer = Framebuffer::new();
        let mut local_state = worker_state.lock().unwrap().clone();
        let mut dirty = false;
        'redraw: loop {
            framebuffer.clear();
            draw(&mut framebuffer, &local_state);
            let prepared = PreparedFramebuffer::prepare_difference(
                &prev_framebuffer,
                &framebuffer,
                DrawMode::DirectUpdateBinary,
            );
            paper.powered_on().draw(&prepared);
            std::mem::swap(&mut prev_framebuffer, &mut framebuffer);
            let mut tries = 0;
            loop {
                FreeRtos.delay_ms(10u32);
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
            }
        }
    });

    // Input loop
    loop {
        press_latch(&buttons.field_cycle, false, || {
            let mut state = state.lock().unwrap();
            state.field = state.field.cycle();
        });
        press_latch(&buttons.backward, true, || {
            let mut state = state.lock().unwrap();
            state.time = adjust(state.field, AdjustDirection::Backward, state.time);
            state.changed = true;
        });
        press_latch(&buttons.forward, true, || {
            let mut state = state.lock().unwrap();
            if matches!(state.field, AdjustField::Store) {
                set_counter(&partition, counter_from_datetime(state.time));
                state.changed = false;
            } else {
                state.time = adjust(state.field, AdjustDirection::Forward, state.time);
                state.changed = true;
            }
        });
        FreeRtos.delay_ms(1u32);
    }
}

fn draw(framebuffer: &mut Framebuffer, state: &State) {
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
        (AdjustField::Hours, 400, 180),
        (AdjustField::Minutes, 560, 180),
        (AdjustField::Days, 290, 300),
        (AdjustField::Months, 450, 300),
        (AdjustField::Years, 650, 300),
        (AdjustField::Store, 480, 430),
    ] {
        framebuffer.text_centered(Paint::Darken, x, y, 94., &part.format(state.time));
        if part == state.field {
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
    if state.changed {
        framebuffer.text_centered(Paint::Darken, 480, 480, 50., "not saved");
    }
}

fn press_latch(pin: &GpioPin<Input>, repeat: bool, mut cb: impl FnMut()) {
    if pin.is_low().unwrap() {
        cb();
        for _ in 0..50 {
            if pin.is_high().unwrap() {
                return;
            }
            FreeRtos.delay_ms(10u32);
        }
        loop {
            if pin.is_high().unwrap() {
                return;
            }
            if repeat {
                cb();
                FreeRtos.delay_ms(200u32);
            } else {
                FreeRtos.delay_ms(10u32);
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum AdjustField {
    Years,
    Months,
    Days,
    Hours,
    Minutes,
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
            AdjustField::Years => AdjustField::Months,
            AdjustField::Months => AdjustField::Days,
            AdjustField::Days => AdjustField::Hours,
            AdjustField::Hours => AdjustField::Minutes,
            AdjustField::Minutes => AdjustField::Store,
            AdjustField::Store => AdjustField::Years,
        }
    }

    fn format(self, datetime: NaiveDateTime) -> String {
        let format_string = match self {
            AdjustField::Years => "%Y",
            AdjustField::Months => "%m",
            AdjustField::Days => "%d",
            AdjustField::Hours => "%H",
            AdjustField::Minutes => "%M",
            AdjustField::Store => "Save",
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
    clamp_datetime_to_counter(date.and_time(time))
}
