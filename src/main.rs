use esp_idf_sys::{self as _, EpdInitOptions_EPD_OPTIONS_DEFAULT, epd_init, epd_set_rotation, EpdRotation_EPD_ROT_LANDSCAPE, epd_clear, epd_clear_area_cycles, epd_full_screen, epd_poweroff, epd_poweron};

fn main() {
    println!("Hello, world!");
    unsafe {
        epd_init(EpdInitOptions_EPD_OPTIONS_DEFAULT);
        epd_set_rotation(EpdRotation_EPD_ROT_LANDSCAPE);
        epd_poweron();
        epd_clear();
        epd_poweroff();
    }
}
