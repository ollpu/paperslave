use rusttype::{self, Font, Point, Scale};

pub const WIDTH: i32 = 960;
pub const HEIGHT: i32 = 540;

pub const WHITE: u8 = u8::MAX;
pub const BLACK: u8 = u8::MIN;

static FONT_DATA: &[u8] = include_bytes!("../font/Lato-Bold.subset.ttf") as &[u8];

pub struct Framebuffer {
    data: Vec<u8>,
}

#[derive(Clone, Copy)]
pub enum Paint {
    Darken,
    Lighten,
}

#[derive(Clone, Copy)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Framebuffer {
    pub fn new() -> Framebuffer {
        Framebuffer {
            data: vec![WHITE; (WIDTH * HEIGHT) as usize],
        }
    }

    pub fn clear(&mut self) {
        self.data.fill(WHITE);
    }

    pub fn inside(&self, x: i32, y: i32) -> bool {
        (0..WIDTH).contains(&x) && (0..HEIGHT).contains(&y)
    }

    pub fn get(&self, x: i32, y: i32) -> u8 {
        let pos = y
            .checked_mul(WIDTH)
            .and_then(|i| i.checked_add(x))
            .and_then(|i| i.try_into().ok())
            .and_then(|i: usize| self.data.get(i))
            .expect("position outside framebuffer");
        *pos
    }

    pub fn set(&mut self, x: i32, y: i32, val: u8) {
        let pos = y
            .checked_mul(WIDTH)
            .and_then(|i| i.checked_add(x))
            .and_then(|i| i.try_into().ok())
            .and_then(|i: usize| self.data.get_mut(i))
            .expect("position outside framebuffer");
        *pos = val;
    }

    pub fn paint(&mut self, paint: Paint, x: i32, y: i32, val: u8) {
        if self.inside(x, y) {
            let before = self.get(x, y);
            let after = match paint {
                Paint::Darken => before.saturating_sub(val),
                Paint::Lighten => before.saturating_add(val),
            };
            self.set(x, y, after);
        }
    }

    pub fn rect(&mut self, paint: Paint, rect: Rect) {
        let Rect { x, y, w, h } = rect;
        for p_x in x..(x + w) {
            for p_y in y..(y + h) {
                self.paint(paint, p_x, p_y, u8::MAX);
            }
        }
    }

    /// Draw text centered horizontally. Vertically aligned to the baseline.
    pub fn text_centered(&mut self, paint: Paint, x: i32, y: i32, size: f32, content: &str) {
        let scale = Scale::uniform(size);

        let glyphs: Vec<_> = FONT
            .layout(content, scale, Point { x: 0., y: 0. })
            .collect();

        let min_x = glyphs
            .first()
            .map(|g| g.pixel_bounding_box().unwrap().min.x)
            .unwrap();
        let max_x = glyphs
            .last()
            .map(|g| g.pixel_bounding_box().unwrap().max.x)
            .unwrap();
        let glyphs_width = max_x - min_x;

        let x = x - glyphs_width / 2 - min_x;

        for glyph in &glyphs {
            if let Some(bounding_box) = glyph.pixel_bounding_box() {
                glyph.draw(|inner_x, inner_y, val| {
                    let actual_x = inner_x as i32 + bounding_box.min.x + x;
                    let actual_y = inner_y as i32 + bounding_box.min.y + y;
                    let val = (val * 255.) as u8;
                    self.paint(paint, actual_x, actual_y, val);
                });
            }
        }
    }
}

lazy_static::lazy_static! {
    static ref FONT: Font<'static> = Font::try_from_bytes(FONT_DATA).expect("failed loading font");
}
