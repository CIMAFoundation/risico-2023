use std::io::Read;

use log::warn;

use crate::common::helpers::RISICOError;

#[derive(Debug, Clone, Copy)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Debug, Clone)]
pub struct Palette {
    bounds: Vec<f32>,
    colors: Vec<Color>,
}

impl Palette {
    // pub fn new(min: f32, max: f32) -> Self {
    //     let mut palette = Self {
    //         bounds: Vec::new(),
    //         colors: Vec::new(),
    //     };

    //     palette.bounds.push(-9999.0);
    //     let mut c = Color {
    //         r: 0,
    //         g: 0,
    //         b: 0,
    //         a: 0,
    //     };
    //     palette.colors.push(c);

    //     let step = (max - min) / 255.0;
    //     for i in 0..255 {
    //         let val = min + i as f32 * step;

    //         c.r = i as u8;
    //         c.g = i as u8;
    //         c.b = i as u8;
    //         c.a = 255;

    //         palette.bounds.push(val);
    //         palette.colors.push(c);
    //     }

    //     palette
    // }

    pub fn load_palette(s_palette_file: &str) -> Result<Self, RISICOError> {
        let ifs = std::fs::File::open(s_palette_file)
            .map_err(|err| format!("cannot open palette file {s_palette_file}: {err}."))?;

        let mut reader = std::io::BufReader::new(ifs);

        let mut contents = String::new();

        reader
            .read_to_string(&mut contents)
            .map_err(|err| format!("cannot read palette file {s_palette_file}: {err}."))?;

        let mut bounds: Vec<f32> = Vec::new();
        let mut colors: Vec<Color> = Vec::new();

        let lines: Vec<&str> = contents.split('\n').collect();
        for line in lines {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 5 {
                warn!("warning skipping line: {}", line);
                continue;
            }
            let val: f32 = parts[0]
                .parse()
                .map_err(|err| format!("Cannot parse {line} {err}"))?;

            let r: u8 = parts[1]
                .parse()
                .map_err(|err| format!("Cannot parse {line} {err}"))?;
            let g: u8 = parts[2]
                .parse()
                .map_err(|err| format!("Cannot parse {line} {err}"))?;
            let b: u8 = parts[3]
                .parse()
                .map_err(|err| format!("Cannot parse {line} {err}"))?;
            let a: u8 = parts[4]
                .parse()
                .map_err(|err| format!("Cannot parse {line} {err}"))?;

            let c = Color { r, g, b, a };
            bounds.push(val);
            colors.push(c);
        }
        Ok(Self { bounds, colors })
    }

    pub fn get_color(&self, val: f32) -> Color {
        let fallback = Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
        if self.bounds.is_empty() || self.colors.is_empty() {
            return fallback;
        }

        for (idx, bounds) in self.bounds.windows(2).enumerate() {
            if val >= bounds[0] && val < bounds[1] {
                return self.colors.get(idx).copied().unwrap_or(fallback);
            }
        }

        self.colors.last().copied().unwrap_or(fallback)
    }
}

#[cfg(test)]
mod tests {
    use super::Palette;
    use std::{fs, path::PathBuf};

    #[test]
    fn malformed_rows_are_skipped_without_panicking_on_lookup() {
        let path: PathBuf =
            std::env::temp_dir().join(format!("risico-palette-test-{}.pal", std::process::id()));
        fs::write(&path, "0 0 0 0 255\n10 255 0 0\n20 0 255 0 255\n")
            .expect("test palette should be writable");

        let palette = Palette::load_palette(path.to_str().expect("valid test path"))
            .expect("palette should load");
        let color = palette.get_color(100.0);
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 255);
        assert_eq!(color.a, 255);

        fs::remove_file(path).expect("test palette should be removable");
    }
}
