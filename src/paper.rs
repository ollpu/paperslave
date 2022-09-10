use esp_idf_hal::{gpio::*, rmt};
use esp_idf_sys::{
    epd_clear, epd_clear_area, epd_clear_area_cycles, epd_deinit, epd_draw_base, epd_full_screen,
    epd_init, epd_poweroff, epd_poweron, epd_set_rotation, epdiy_ED047TC1,
    EpdDrawError_EPD_DRAW_SUCCESS, EpdDrawMode, EpdDrawMode_MODE_DU,
    EpdDrawMode_MODE_EPDIY_BLACK_TO_GL16, EpdDrawMode_MODE_EPDIY_WHITE_TO_GL16,
    EpdDrawMode_MODE_GC16, EpdDrawMode_MODE_GL16, EpdDrawMode_MODE_PACKING_1PPB_DIFFERENCE,
    EpdDrawMode_MODE_PACKING_2PPB, EpdDrawMode_PREVIOUSLY_WHITE,
    EpdInitOptions_EPD_OPTIONS_DEFAULT, EpdRotation_EPD_ROT_LANDSCAPE,
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

#[derive(Clone, Copy)]
#[repr(u32)]
pub enum DrawMode {
    DirectUpdateBinary = EpdDrawMode_MODE_DU,
    Flashing = EpdDrawMode_MODE_GC16,
    NonFlashing = EpdDrawMode_MODE_GL16,
    FromWhiteQuick = EpdDrawMode_MODE_EPDIY_WHITE_TO_GL16,
    FromBlackQuick = EpdDrawMode_MODE_EPDIY_BLACK_TO_GL16,
}

pub struct PreparedFramebuffer {
    packed: Vec<u8>,
    mode: EpdDrawMode,
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

    pub fn quick_clear(&mut self) {
        unsafe {
            epd_clear_area_cycles(epd_full_screen(), 1, 40);
        }
    }

    pub fn draw(&mut self, prepared: &PreparedFramebuffer) {
        unsafe {
            let ret = epd_draw_base(
                epd_full_screen(),
                prepared.packed.as_ptr(),
                epd_full_screen(),
                prepared.mode,
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

impl PreparedFramebuffer {
    pub fn prepare(framebuffer: &Framebuffer, draw_mode: DrawMode) -> PreparedFramebuffer {
        // TODO: use 8 bits per byte for binary mode?
        assert!(WIDTH % 2 == 0);
        let mut packed = vec![0; (WIDTH / 2 * HEIGHT) as usize];
        for y in 0..HEIGHT {
            for x in 0..WIDTH / 2 {
                let packed_idx = (y * (WIDTH / 2) + x) as usize;
                let l = framebuffer.get(2 * x, y);
                let r = framebuffer.get(2 * x + 1, y);
                let (l, r) = if matches!(draw_mode, DrawMode::DirectUpdateBinary) {
                    (15 * (l >> 7), 15 * (r >> 7))
                } else {
                    (l >> 4, r >> 4)
                };
                let combined = r << 4 | l;
                packed[packed_idx] = combined;
            }
        }
        PreparedFramebuffer {
            packed,
            mode: draw_mode as EpdDrawMode
                | EpdDrawMode_PREVIOUSLY_WHITE
                | EpdDrawMode_MODE_PACKING_2PPB,
        }
    }

    pub fn prepare_difference(
        from_framebuffer: &Framebuffer,
        to_framebuffer: &Framebuffer,
        draw_mode: DrawMode,
    ) -> PreparedFramebuffer {
        let mut packed = vec![0; (WIDTH * HEIGHT) as usize];
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let packed_idx = (y * WIDTH + x) as usize;
                let from = from_framebuffer.get(x, y);
                let to = to_framebuffer.get(x, y);
                let (from, to) = if matches!(draw_mode, DrawMode::DirectUpdateBinary) {
                    (15 * (from >> 7), 15 * (to >> 7))
                } else {
                    (from >> 4, to >> 4)
                };
                let combined = to << 4 | from;
                packed[packed_idx] = combined;
            }
        }
        PreparedFramebuffer {
            packed,
            mode: draw_mode as EpdDrawMode | EpdDrawMode_MODE_PACKING_1PPB_DIFFERENCE,
        }
    }
}
