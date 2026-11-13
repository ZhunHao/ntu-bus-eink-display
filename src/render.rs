//! Draws the 250x122 black/white board into a framebuffer.

use chrono::{DateTime, FixedOffset};
use embedded_graphics::{
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{CornerRadii, Line, PrimitiveStyle, Rectangle, RoundedRectangle},
};
use u8g2_fonts::{
    fonts,
    types::{FontColor, HorizontalAlignment, VerticalPosition},
    FontRenderer,
};

pub const WIDTH: u32 = 250;
pub const HEIGHT: u32 = 122;
const HEADER_H: i32 = 19;

const BLACK: BinaryColor = BinaryColor::On;
const WHITE: BinaryColor = BinaryColor::Off;

/// Landscape 250x122 1-bit image. `true` = black.
pub struct Framebuffer {
    pixels: Vec<bool>,
}

impl Framebuffer {
    pub fn new() -> Self {
        Self { pixels: vec![false; (WIDTH * HEIGHT) as usize] }
    }

    pub fn get(&self, x: u32, y: u32) -> bool {
        self.pixels[(y * WIDTH + x) as usize]
    }

    /// Bytes in the panel's native layout: portrait 122x250, 16 bytes per row,
    /// MSB first, 1 = white. Matches Waveshare's getbuffer() for a landscape image
    /// (rotate 90 degrees clockwise). `flip` turns the image upside down.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))] // only the e-paper uses it
    pub fn to_epd(&self, flip: bool) -> Vec<u8> {
        const ROW_BYTES: usize = (HEIGHT as usize).div_ceil(8);
        let mut buf = vec![0xFF; ROW_BYTES * WIDTH as usize];
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let (sx, sy) = if flip { (WIDTH - 1 - x, HEIGHT - 1 - y) } else { (x, y) };
                if self.get(sx, sy) {
                    let (px, py) = ((HEIGHT - 1 - y) as usize, x as usize);
                    buf[py * ROW_BYTES + px / 8] &= !(0x80 >> (px % 8));
                }
            }
        }
        buf
    }

    /// 8-bit greyscale pixels, each one drawn `scale` times across and down.
    fn grey(&self, scale: u32) -> Vec<u8> {
        let (w, h) = (WIDTH * scale, HEIGHT * scale);
        let mut data = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                data.push(if self.get(x / scale, y / scale) { 0 } else { 255 });
            }
        }
        data
    }

    /// Greyscale PNG, scaled up for easy viewing.
    pub fn write_png(&self, path: &str, scale: u32) -> crate::Result<()> {
        let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(path)?), WIDTH * scale, HEIGHT * scale);
        enc.set_color(png::ColorType::Grayscale);
        enc.write_header()?.write_image_data(&self.grey(scale))?;
        Ok(())
    }
}

impl OriginDimensions for Framebuffer {
    fn size(&self) -> Size {
        Size::new(WIDTH, HEIGHT)
    }
}

impl DrawTarget for Framebuffer {
    type Color = BinaryColor;
    type Error = core::convert::Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<BinaryColor>>>(&mut self, pixels: I) -> Result<(), Self::Error> {
        for Pixel(p, c) in pixels {
            if p.x >= 0 && p.y >= 0 && (p.x as u32) < WIDTH && (p.y as u32) < HEIGHT {
                self.pixels[(p.y as u32 * WIDTH + p.x as u32) as usize] = c.is_on();
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Shuttle, // black badge
    Public,  // outlined badge
}

pub struct Row {
    pub label: String,
    pub dest: Option<String>,
    pub kind: Kind,
    pub arrivals: Vec<DateTime<FixedOffset>>, // soonest first
    /// No usable data: the fetch failed and there's nothing recent enough to show instead
    pub error: bool,
}

impl Row {
    /// Buses still to come at `now`. A bus stays listed until a minute after its arrival time,
    /// since estimates run late.
    pub fn upcoming(&self, now: DateTime<FixedOffset>) -> Vec<DateTime<FixedOffset>> {
        self.arrivals.iter().copied().filter(|t| (*t - now).num_seconds() > -60).collect()
    }
}

/// "Arr" inside a minute; otherwise whole minutes rounded down, or the clock time.
pub fn fmt_eta(arrival: DateTime<FixedOffset>, now: DateTime<FixedOffset>, clock: bool) -> String {
    let seconds = (arrival - now).num_seconds();
    if seconds < 60 {
        "Arr".into()
    } else if clock {
        arrival.format("%H:%M").to_string()
    } else {
        (seconds / 60).to_string()
    }
}

const INK: FontColor<BinaryColor> = FontColor::Transparent(BLACK);

/// Route badge: filled for campus shuttles, outlined for public buses, centred on `cy`.
fn draw_badge(d: &mut Framebuffer, font: &FontRenderer, row: &Row, cy: i32, size: Size) {
    let (bw, bh) = (size.width as i32, size.height as i32);
    let shape = RoundedRectangle::new(
        Rectangle::new(Point::new(3, cy - bh / 2), size),
        CornerRadii::new(Size::new(5, 5)),
    );
    let label_color = if row.kind == Kind::Shuttle {
        shape.into_styled(PrimitiveStyle::with_fill(BLACK)).draw(d).unwrap();
        FontColor::Transparent(WHITE)
    } else {
        shape.into_styled(PrimitiveStyle::with_stroke(BLACK, 2)).draw(d).unwrap();
        INK
    };
    font.render_aligned(row.label.as_str(), Point::new(3 + bw / 2, cy), VerticalPosition::Center, HorizontalAlignment::Center, label_color, d)
        .unwrap();
}

pub fn render(rows: &[Row], title: &str, now: DateTime<FixedOffset>, clock: bool) -> Framebuffer {
    let header = FontRenderer::new::<fonts::u8g2_font_helvB14_tr>().with_ignore_unknown_chars(true);
    let (left, right) = (HorizontalAlignment::Left, HorizontalAlignment::Right);
    let center = VerticalPosition::Center;

    let mut fb = Framebuffer::new();
    let d = &mut fb;
    let w = WIDTH as i32;

    // Infallible draw target, and the fonts skip characters they lack (they are ASCII only),
    // so the unwraps below cannot fail
    header.render_aligned(title.to_uppercase().as_str(), Point::new(3, 9), center, left, INK, d).unwrap();
    header
        .render_aligned(now.format("%H:%M").to_string().as_str(), Point::new(w - 3, 9), center, right, INK, d)
        .unwrap();
    Rectangle::new(Point::new(0, HEADER_H - 2), Size::new(WIDTH, 2))
        .into_styled(PrimitiveStyle::with_fill(BLACK))
        .draw(d)
        .unwrap();

    let row_h = (HEIGHT as i32 - HEADER_H) / rows.len().max(1) as i32;
    for (i, row) in rows.iter().enumerate() {
        let top = HEADER_H + i as i32 * row_h;
        if i > 0 {
            Line::new(Point::new(0, top), Point::new(w, top)).into_styled(PrimitiveStyle::with_stroke(BLACK, 1)).draw(d).unwrap();
        }
        if rows.len() <= 2 {
            draw_roomy_row(d, row, top, row_h, now, clock);
        } else {
            draw_compact_row(d, row, top + row_h / 2, now, clock);
        }
    }
    fb
}

/// Three or more rows: one line per route, "3 min ... next 8 min".
fn draw_compact_row(d: &mut Framebuffer, row: &Row, mid: i32, now: DateTime<FixedOffset>, clock: bool) {
    let badge = FontRenderer::new::<fonts::u8g2_font_helvB14_tr>().with_ignore_unknown_chars(true);
    let big = FontRenderer::new::<fonts::u8g2_font_helvB24_tr>().with_ignore_unknown_chars(true);
    let small = FontRenderer::new::<fonts::u8g2_font_helvR14_tr>().with_ignore_unknown_chars(true);
    let next_font = FontRenderer::new::<fonts::u8g2_font_helvB14_tr>().with_ignore_unknown_chars(true);
    let note = FontRenderer::new::<fonts::u8g2_font_helvR10_tr>().with_ignore_unknown_chars(true);
    let (center, baseline) = (VerticalPosition::Center, VerticalPosition::Baseline);
    let w = WIDTH as i32;
    let text_x = 78;

    draw_badge(d, &badge, row, mid, Size::new(64, 23));
    let etas = row.upcoming(now);
    if row.error || etas.is_empty() {
        let msg = if row.error { "no data" } else { "no bus" };
        small.render_aligned(msg, Point::new(text_x, mid), center, HorizontalAlignment::Left, INK, d).unwrap();
        return;
    }

    let first = fmt_eta(etas[0], now, clock);
    let dims = big.render(first.as_str(), Point::new(text_x, mid), center, INK, d).unwrap();
    if !clock && first != "Arr" {
        note.render(" min", Point::new(text_x + dims.advance.x, mid + 5), center, INK, d).unwrap();
    }
    if let Some(&next) = etas.get(1) {
        // "next 8 min", right-aligned, styled like the first ETA but smaller
        let num = fmt_eta(next, now, clock);
        let unit = if clock || num == "Arr" { "" } else { " min" };
        let width = |f: &FontRenderer, s: &str| {
            f.get_rendered_dimensions(s, Point::zero(), baseline).map_or(0, |r| r.advance.x)
        };
        let y = mid + 5;
        let mut x = w - 5 - width(&note, "next ") - width(&next_font, &num) - width(&note, unit);
        x += note.render("next ", Point::new(x, y), baseline, INK, d).unwrap().advance.x;
        x += next_font.render(num.as_str(), Point::new(x, y), baseline, INK, d).unwrap().advance.x;
        note.render(unit, Point::new(x, y), baseline, INK, d).unwrap();
    }
}

/// Two rows or fewer, MTA countdown style: badge with the destination underneath,
/// then the next bus large and the one after medium, each with "min" stacked below.
fn draw_roomy_row(d: &mut Framebuffer, row: &Row, top: i32, row_h: i32, now: DateTime<FixedOffset>, clock: bool) {
    let badge = FontRenderer::new::<fonts::u8g2_font_helvB14_tr>().with_ignore_unknown_chars(true);
    let dest = FontRenderer::new::<fonts::u8g2_font_helvR12_tr>().with_ignore_unknown_chars(true);
    let big = FontRenderer::new::<fonts::u8g2_font_helvB24_tr>().with_ignore_unknown_chars(true);
    let medium = FontRenderer::new::<fonts::u8g2_font_helvB18_tr>().with_ignore_unknown_chars(true);
    let next_small = FontRenderer::new::<fonts::u8g2_font_helvB14_tr>().with_ignore_unknown_chars(true);
    let small = FontRenderer::new::<fonts::u8g2_font_helvR14_tr>().with_ignore_unknown_chars(true);
    let unit = FontRenderer::new::<fonts::u8g2_font_helvR08_tr>().with_ignore_unknown_chars(true);
    let (baseline, h_center) = (VerticalPosition::Baseline, HorizontalAlignment::Center);
    let w = WIDTH as i32;
    let mid = top + row_h / 2;

    match &row.dest {
        Some(text) => {
            draw_badge(d, &badge, row, top + 15, Size::new(56, 22));
            dest.render(format!("> {text}").as_str(), Point::new(4, top + 42), baseline, INK, d).unwrap();
        }
        None => draw_badge(d, &badge, row, mid, Size::new(56, 22)),
    }

    // Baseline that centres the drawn ink of `s` on the row (fonts' own centring leaves room
    // for descenders and accents, which sits digits a few pixels high)
    let centred = |f: &FontRenderer, s: &str| {
        f.get_rendered_dimensions(s, Point::zero(), baseline)
            .ok()
            .and_then(|r| r.bounding_box)
            .map_or(mid, |b| mid - (2 * b.top_left.y + b.size.height as i32) / 2)
    };

    let etas = row.upcoming(now);
    if row.error || etas.is_empty() {
        let msg = if row.error { "no data" } else { "no bus" };
        small.render_aligned(msg, Point::new(w - 5, centred(&small, msg)), baseline, HorizontalAlignment::Right, INK, d).unwrap();
        return;
    }

    // Clock times ("17:33") are about three digits wider than minutes, so they drop a size
    let slots = if clock { [(&medium, 158), (&next_small, 222)] } else { [(&big, 166), (&medium, 221)] };
    const UNIT_GAP: i32 = 11; // baseline of "min" below the number's baseline
    // The first time is centred on the row; later ones share its bottom edge, because a
    // smaller block centred beside a bigger one reads as sitting high
    let mut bottom = None;
    for (&eta, (font, x)) in etas.iter().zip(slots) {
        let text = fmt_eta(eta, now, clock);
        let with_unit = !clock && text != "Arr";
        // Measure the ink of the number (and "min" under it), then centre that block on the row
        let ink_rows = |f: &FontRenderer, s: &str, y: i32| {
            f.get_rendered_dimensions(s, Point::new(x, y), baseline)
                .ok()
                .and_then(|r| r.bounding_box)
                .map(|b| (b.top_left.y, b.top_left.y + b.size.height as i32))
        };
        let mut span = ink_rows(font, &text, 0).unwrap_or((0, 0));
        if with_unit {
            if let Some((t, b)) = ink_rows(&unit, "min", UNIT_GAP) {
                span = (span.0.min(t), span.1.max(b));
            }
        }
        let y = match bottom {
            Some(b) => b - span.1,
            None => mid - (span.0 + span.1) / 2,
        };
        bottom.get_or_insert(y + span.1);
        font.render_aligned(text.as_str(), Point::new(x, y), baseline, h_center, INK, d).unwrap();
        if with_unit {
            unit.render_aligned("min", Point::new(x, y + UNIT_GAP), baseline, h_center, INK, d).unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-11-05T17:30:00+08:00").unwrap()
    }

    /// `secs` seconds after `now()`
    fn at(secs: i64) -> DateTime<FixedOffset> {
        now() + chrono::Duration::seconds(secs)
    }

    fn row(arrivals: Vec<DateTime<FixedOffset>>) -> Row {
        Row { label: "Red".into(), dest: None, kind: Kind::Shuttle, arrivals, error: false }
    }

    #[test]
    fn eta_rounds_down_with_arr_inside_a_minute() {
        assert_eq!(fmt_eta(at(59), now(), false), "Arr");
        assert_eq!(fmt_eta(at(-30), now(), false), "Arr");
        assert_eq!(fmt_eta(at(60), now(), false), "1");
        assert_eq!(fmt_eta(at(299), now(), false), "4");
        assert_eq!(fmt_eta(at(242), now(), true), "17:34");
        // a bus that's due or slightly late shows "Arr", not a clock time already gone
        assert_eq!(fmt_eta(at(-30), now(), true), "Arr");
        assert_eq!(fmt_eta(at(30), now(), true), "Arr");
    }

    #[test]
    fn buses_drop_off_a_minute_after_arriving() {
        let r = row(vec![at(-90), at(-30), at(300)]);
        assert_eq!(r.upcoming(now()), vec![at(-30), at(300)]);
        // the same data redrawn two minutes later
        assert_eq!(r.upcoming(at(120)), vec![at(300)]);
    }

    /// Every layout, compared pixel for pixel with the sample boards in docs/images, so a
    /// change that moves anything shows up here. After changing a layout on purpose, redraw
    /// them with `UPDATE_SAMPLES=1 cargo test sample_boards` and look at the PNGs.
    #[test]
    fn sample_boards_match_docs_images() {
        let rows = || {
            vec![
                Row { label: "Red".into(), dest: Some("Campus Loop".into()), kind: Kind::Shuttle, arrivals: vec![at(180), at(480)], error: false },
                Row { label: "Grey".into(), dest: None, kind: Kind::Shuttle, arrivals: vec![at(30), at(960)], error: false },
                Row { label: "199".into(), dest: Some("Boon Lay Int".into()), kind: Kind::Public, arrivals: vec![at(720), at(1380)], error: false },
            ]
        };
        let mut two = rows();
        two.remove(1); // Grey hidden outside exam periods
        let edge = vec![
            Row { label: "Red".into(), dest: Some("Campus Loop".into()), kind: Kind::Shuttle, arrivals: vec![at(20), at(420)], error: false },
            Row { label: "199".into(), dest: Some("Boon Lay Int".into()), kind: Kind::Public, arrivals: vec![], error: true },
        ];
        let boards = [
            ("sample-three-rows", render(&rows(), "Hall 11", now(), false)),
            ("sample-three-rows-clock", render(&rows(), "Hall 11", now(), true)),
            ("sample-two-rows", render(&two, "Hall 11", now(), false)),
            ("sample-two-rows-clock", render(&two, "Hall 11", now(), true)),
            ("sample-two-rows-no-data", render(&edge, "Hall 11", now(), false)),
        ];
        let update = std::env::var_os("UPDATE_SAMPLES").is_some();
        let mut changed = vec![];
        for (name, fb) in boards {
            let path = format!("docs/images/{name}.png");
            if update {
                fb.write_png(&path, 3).unwrap();
            } else if read_png(&path).ok() != Some(fb.grey(3)) {
                let drawn = format!("target/{name}.png");
                fb.write_png(&drawn, 3).unwrap();
                changed.push(format!("{path} (now draws as {drawn})"));
            }
        }
        assert!(changed.is_empty(), "layouts no longer match their samples: {changed:#?}");
    }

    /// The pixels of an 8-bit greyscale PNG.
    fn read_png(path: &str) -> crate::Result<Vec<u8>> {
        let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path)?)).read_info()?;
        let mut data = vec![0; reader.output_buffer_size().ok_or("PNG too large")?];
        let frame = reader.next_frame(&mut data)?;
        if (frame.color_type, frame.bit_depth) != (png::ColorType::Grayscale, png::BitDepth::Eight) {
            return Err("not an 8-bit greyscale PNG".into());
        }
        data.truncate(frame.buffer_size());
        Ok(data)
    }

    #[test]
    fn text_the_fonts_cannot_draw_is_skipped() {
        let mut rows = vec![row(vec![at(180)])];
        rows[0].label = "Café".into();
        rows[0].dest = Some("南洋 → Hall".into());
        render(&rows, "Hall 11 – é", now(), false); // must not panic
    }

    #[test]
    fn epd_buffer_layout() {
        let mut fb = Framebuffer::new();
        Pixel(Point::new(0, 0), BLACK).draw(&mut fb).unwrap();
        let buf = fb.to_epd(false);
        assert_eq!(buf.len(), 16 * 250);
        // landscape (0,0) -> portrait (121, 0): byte 15, bit 121 % 8 = 1
        assert_eq!(buf[15], !(0x80 >> 1));
        assert_eq!(buf.iter().filter(|b| **b != 0xFF).count(), 1);
        // flipped, it lands in the opposite corner: portrait (0, 249), the last row's first bit
        let flipped = fb.to_epd(true);
        assert_eq!(flipped[249 * 16], !0x80);
        assert_eq!(flipped.iter().filter(|b| **b != 0xFF).count(), 1);
    }
}
