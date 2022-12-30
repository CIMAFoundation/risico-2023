use std::io::Read;

use crate::library::config::models::ConfigError;

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
    pub fn new(min: f32, max: f32) -> Self {
        let mut palette = Self {
            bounds: Vec::new(),
            colors: Vec::new(),
        };

        palette.bounds.push(-9999.0);
        let mut c = Color { r: 0, g: 0, b: 0, a: 0 };
        palette.colors.push(c);

        let step = (max - min) / 255.0;
        for i in 0..255 {
            let val = min + i as f32 * step;

            c.r = i as u8;
            c.g = i as u8;
            c.b = i as u8;
            c.a = 255;

            palette.bounds.push(val);
            palette.colors.push(c);
        }

        palette
    }

    pub fn load_palette(s_palette_file: &str) -> Result<Self, ConfigError> {
        let ifs = std::fs::File::open(s_palette_file)
            .map_err(|err| format!("cannot open palette file {s_palette_file}: {err}."))?;

        let mut reader = std::io::BufReader::new(ifs);
        
        let mut contents = String::new();
        
        reader.read_to_string(&mut contents)
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
            let mut parts = line.split_whitespace();
            let val: f32 = parts.next().unwrap().parse().unwrap();
            let r: u8 = parts.next().unwrap().parse().unwrap();
            let g: u8 = parts.next().unwrap().parse().unwrap();
            let b: u8 = parts.next().unwrap().parse().unwrap();
            let a: u8 = parts.next().unwrap().parse().unwrap();

            let c = Color { r, g, b, a };
            bounds.push(val);
            colors.push(c);
        }
        Ok(Self {
            bounds,
            colors,
        })
    }

    pub fn get_color(&self, val: f32) -> Color {
        for (idx, bound) in self.bounds.iter().enumerate().take(self.bounds.len() - 1) {
            if val >= *bound && val < self.bounds[idx + 1] {
                return self.colors[idx];
            }
        }

        self.colors[self.bounds.len() - 1]
    }
}
