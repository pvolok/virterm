use ab_glyph::ScaleFont;
use anyhow::Result;
use image::Rgb;

pub fn dump_png(screen: &vt100::Screen, path: &str) -> Result<()> {
  let px = 43.0;
  let (h, w) = screen.size();

  let fonts = {
    let regular = include_bytes!("fonts/JetBrainsMono-Regular.ttf") as &[u8];
    let bold = include_bytes!("fonts/JetBrainsMono-Bold.ttf") as &[u8];
    let italic = include_bytes!("fonts/JetBrainsMono-Italic.ttf") as &[u8];
    let bold_italic =
      include_bytes!("fonts/JetBrainsMono-BoldItalic.ttf") as &[u8];

    let fonts = [regular, bold, italic, bold_italic];
    let fonts = fonts.map(|font| {
      let font = ab_glyph::FontRef::try_from_slice(font).unwrap();
      let font = ab_glyph::Font::into_scaled(font, px);
      font
    });

    fonts
  };

  let canon = fonts[0].scaled_glyph('a');
  let canon_b = fonts[0].glyph_bounds(&canon);
  let ch_w = canon_b.max.x;
  let ch_h = fonts[0].height();

  let mut canvas = image::RgbImage::new(
    (w as f32 * ch_w).round() as u32,
    (h as f32 * ch_h).round() as u32,
  );

  fn vt_color_to_rgb(from: vt100::Color) -> Option<[u8; 3]> {
    let color = match from {
      vt100::Color::Default => return None,
      vt100::Color::Idx(idx) => {
        let (r, g, b) = ansi_colours::rgb_from_ansi256(idx);
        [r, g, b]
      }
      vt100::Color::Rgb(r, g, b) => [r, g, b],
    };
    Some(color)
  }

  let def_bg = [10, 10, 50];
  let def_fg = [240, 240, 240];

  for row in 0..h {
    for col in 0..w {
      let cell = screen.cell(row, col).unwrap();
      let fg = vt_color_to_rgb(cell.fgcolor()).unwrap_or(def_fg);
      let bg = vt_color_to_rgb(cell.bgcolor()).unwrap_or(def_bg);

      let x0 = (col as f32 * ch_w).round() as u32;
      let y0 = (row as f32 * ch_h).round() as u32;
      for y in y0..(y0 + ch_h as u32) {
        for x in x0..(x0 + ch_w as u32) {
          canvas.put_pixel(x, y, Rgb(bg));
        }
      }

      if let Some(ch) = cell.contents().chars().next() {
        let font = match (cell.bold(), cell.italic()) {
          (false, false) => &fonts[0],
          (true, false) => &fonts[1],
          (false, true) => &fonts[2],
          (true, true) => &fonts[3],
        };
        let glyph = fonts[0].scaled_glyph(ch);
        let outline = font.outline_glyph(glyph);

        if let Some(outline) = outline {
          outline.draw(|dx, dy, c| {
            let x = (col as f32 * ch_w) + dx as f32;
            let y = (row as f32 * ch_h) + dy as f32;

            let y = y + outline.px_bounds().min.y + font.ascent();
            let x = x + outline.px_bounds().min.x;

            if x >= 0.0
              && x < canvas.width() as f32
              && y >= 0.0
              && y < canvas.height() as f32
            {
              let x = x.round() as u32;
              let y = y.round() as u32;

              let pixel = canvas.get_pixel(x, y);
              let pixel = pixel.0.map(|x| x as f32);
              let color = fg.map(|x| x as f32);

              let color = color
                .into_iter()
                .zip(pixel)
                .map(|(top, bot)| (top * c + bot * (1.0 - c)) as u8)
                .collect::<Vec<_>>();
              let color = [color[0], color[1], color[2]];

              canvas.put_pixel(x as u32, y as u32, Rgb(color));
            }
          });
        }
      }
    }
  }

  canvas.save(path)?;

  Ok(())
}

#[allow(dead_code)]
fn debug_font_metrics(font: &ab_glyph::PxScaleFont<&ab_glyph::FontRef>) {
  for ch in ['M', '│', '─', '█'] {
    let gl = font.scaled_glyph(ch);
    println!("---------- {}", ch);
    let bounds = font.glyph_bounds(&gl);
    let outline = font.outline_glyph(gl.clone()).unwrap();
    println!(
      "asc-desc:{} f.h:{} b.w:{} b.h:{} out.w:{} out.h:{}",
      font.ascent() - font.descent(),
      font.height(),
      bounds.width(),
      bounds.height(),
      outline.px_bounds().width(),
      outline.px_bounds().height(),
    );
    println!(
      "bounds min:[{}:{}] max:[{}:{}]",
      bounds.min.x, bounds.min.y, bounds.max.x, bounds.max.y,
    );
    println!(
      "px_bounds min:[{}:{}] max:[{}:{}]",
      outline.px_bounds().min.x,
      outline.px_bounds().min.y,
      outline.px_bounds().max.x,
      outline.px_bounds().max.y,
    );
    println!("adv h:{}", font.h_advance(gl.id));
  }
  println!("----");
}
