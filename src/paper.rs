use esp_idf_hal::{gpio::*, rmt};
use esp_idf_sys::{
    epd_clear, epd_clear_area, epd_init, epd_poweroff, epd_poweron, epd_set_rotation,
    EpdInitOptions_EPD_OPTIONS_DEFAULT, EpdRotation_EPD_ROT_LANDSCAPE,
};

pub use esp_idf_sys::EpdRect;

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

pub struct PaperPowerOn<'a>(&'a mut Paper);

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
}

impl<'a> Drop for PaperPowerOn<'a> {
    fn drop(&mut self) {
        unsafe {
            epd_poweroff();
        }
    }
}
