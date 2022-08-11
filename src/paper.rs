use esp_idf_hal::{gpio::*, rmt};
use esp_idf_sys::{
    c_types, epd_clear, epd_clear_area, epd_deinit, epd_full_screen, epd_init, epd_poweroff,
    epd_poweron, epd_set_rotation, epdiy_ED047TC1, EpdDrawError, EpdDrawError_EPD_DRAW_SUCCESS,
    EpdDrawMode, EpdDrawMode_MODE_EPDIY_WHITE_TO_GL16, EpdDrawMode_MODE_PACKING_2PPB,
    EpdDrawMode_PREVIOUSLY_WHITE, EpdInitOptions_EPD_OPTIONS_DEFAULT,
    EpdRotation_EPD_ROT_LANDSCAPE, EpdWaveform,
};

pub use esp_idf_sys::EpdRect;

use crate::fb::{Framebuffer, HEIGHT, WIDTH};

pub struct PaperPeripherals {
    pub gpio0: Gpio0<Unknown>,
    pub gpio2: Gpio2<Unknown>,
    pub gpio4: Gpio4<Unknown>,
    pub gpio5: Gpio5<Unknown>,
    pub gpio15: Gpio15<Unknown>,
    pub gpio18: Gpio18<Unknown>,
    pub gpio19: Gpio19<Unknown>,
    pub gpio21: Gpio21<Unknown>,
    pub gpio22: Gpio22<Unknown>,
    pub gpio23: Gpio23<Unknown>,
    pub gpio25: Gpio25<Unknown>,
    pub gpio26: Gpio26<Unknown>,
    pub gpio27: Gpio27<Unknown>,
    pub gpio32: Gpio32<Unknown>,
    pub gpio33: Gpio33<Unknown>,
    pub rmt_channel1: rmt::CHANNEL1,
    // also uses i2s, not sure how that maps to the ones exposed in esp-idf-hal
}

pub struct Paper(PaperPeripherals);

impl Paper {
    pub fn init(peripherals: PaperPeripherals) -> Paper {
        unsafe {
            epd_init(EpdInitOptions_EPD_OPTIONS_DEFAULT);
            epd_set_rotation(EpdRotation_EPD_ROT_LANDSCAPE);
        }
        Paper(peripherals)
    }

    pub fn powered_on(&mut self) -> PaperPowerOn<'_> {
        unsafe {
            epd_poweron();
        }
        PaperPowerOn(self)
    }
}

impl Drop for Paper {
    fn drop(&mut self) {
        unsafe {
            epd_deinit();
        }
    }
}

pub struct PaperPowerOn<'a>(&'a mut Paper);

extern "C" {
    /// XXX: Circumvent broken ABI
    /// https://github.com/esp-rs/rust/issues/18
    fn epd_draw_base(
        area: EpdRect,
        data: *const u8,
        _unused: c_types::c_int,
        crop_to: EpdRect,
        mode: EpdDrawMode,
        temperature: c_types::c_int,
        drawn_lines: *const bool,
        waveform: *const EpdWaveform,
    ) -> EpdDrawError;
}

impl<'a> PaperPowerOn<'a> {
    pub fn clear(&mut self) {
        unsafe {
            epd_clear();
        }
    }

    pub fn clear_area(&mut self, area: EpdRect) {
        unsafe {
            epd_clear_area(area);
        }
    }

    pub fn draw_framebuffer(&mut self, framebuffer: &Framebuffer) {
        assert!(WIDTH % 2 == 0);
        let mut packed = vec![0; (WIDTH / 2 * HEIGHT) as usize];
        for y in 0..HEIGHT {
            for x in 0..WIDTH / 2 {
                let packed_idx = (y * (WIDTH / 2) + x) as usize;
                let l = framebuffer.get(2 * x, y) >> 4;
                let r = framebuffer.get(2 * x + 1, y) >> 4;
                let combined = r << 4 | l;
                packed[packed_idx] = combined;
            }
        }
        unsafe {
            let ret = epd_draw_base(
                epd_full_screen(),
                packed.as_ptr(),
                0,
                epd_full_screen(),
                EpdDrawMode_MODE_EPDIY_WHITE_TO_GL16
                    | EpdDrawMode_MODE_PACKING_2PPB
                    | EpdDrawMode_PREVIOUSLY_WHITE,
                24,
                core::ptr::null(),
                &epdiy_ED047TC1 as *const _,
            );
            if ret != EpdDrawError_EPD_DRAW_SUCCESS {
                panic!("epd_draw_base failed with error code {ret}");
            }
        }
    }
}

impl<'a> Drop for PaperPowerOn<'a> {
    fn drop(&mut self) {
        unsafe {
            epd_poweroff();
        }
    }
}
