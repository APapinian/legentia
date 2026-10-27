/// 8 bit gri tonlu çizim yüzeyi. 0 siyah, 255 beyaz.
pub struct Canvas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![255; (width * height) as usize],
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn fill(&mut self, gray: u8) {
        self.pixels.fill(gray);
    }

    /// Ekran dışına taşan kısım kırpılır.
    pub fn fill_rect(&mut self, x: i32, y: i32, w: u32, h: u32, gray: u8) {
        let x0 = x.clamp(0, self.width as i32) as u32;
        let y0 = y.clamp(0, self.height as i32) as u32;
        let x1 = (x + w as i32).clamp(0, self.width as i32) as u32;
        let y1 = (y + h as i32).clamp(0, self.height as i32) as u32;
        for row in y0..y1 {
            let start = (row * self.width + x0) as usize;
            let end = (row * self.width + x1) as usize;
            self.pixels[start..end].fill(gray);
        }
    }

    /// Mevcut pikseli daha koyu olanla değiştirir (beyaz zemine siyah metin).
    pub(crate) fn darken(&mut self, x: i32, y: i32, gray: u8) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let i = (y as u32 * self.width + x as u32) as usize;
        self.pixels[i] = self.pixels[i].min(gray);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_rect_clips_to_canvas() {
        let mut c = Canvas::new(10, 10);
        c.fill_rect(-5, 8, 20, 20, 0);
        let black = c.pixels().iter().filter(|&&p| p == 0).count();
        assert_eq!(black, 10 * 2);
    }

    #[test]
    fn darken_ignores_out_of_bounds() {
        let mut c = Canvas::new(2, 2);
        c.darken(-1, 0, 0);
        c.darken(2, 1, 0);
        assert!(c.pixels().iter().all(|&p| p == 255));
    }
}
