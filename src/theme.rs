#![allow(dead_code)]

use ratatui::style::Color;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const GRADIENT_STEPS: usize = 101;
const DEFAULT_THEME: &str = "ltop";
pub const DEFAULT_UPDATE_MS: u64 = 2_000;
pub const MIN_UPDATE_MS: u64 = 100;
pub const MAX_UPDATE_MS: u64 = 86_400_000;

pub fn validate_update_ms(update_ms: u64) -> Result<u64, String> {
    if update_ms < MIN_UPDATE_MS {
        return Err(format!(
            "update interval must be at least {MIN_UPDATE_MS} ms"
        ));
    }
    if update_ms > MAX_UPDATE_MS {
        return Err(format!(
            "update interval must not exceed {MAX_UPDATE_MS} ms (24 hours)"
        ));
    }
    Ok(update_ms)
}

pub fn parse_update_ms(value: &str) -> Result<u64, String> {
    let update_ms = value.parse::<u64>().map_err(|_| {
        format!("update interval must be an integer in milliseconds, got {value:?}")
    })?;
    validate_update_ms(update_ms)
}

const BUNDLED_THEMES: &[(&str, &str)] = &[
    ("Tokyo Night", include_str!("../themes/tokyo-night.theme")),
    ("Gruvbox Dark", include_str!("../themes/gruvbox-dark.theme")),
    ("Nord", include_str!("../themes/nord.theme")),
    ("Dracula", include_str!("../themes/dracula.theme")),
    (
        "Solarized Light",
        include_str!("../themes/solarized-light.theme"),
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rgb {
    red: u8,
    green: u8,
    blue: u8,
}

impl Rgb {
    const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    fn parse(value: &str) -> Result<Self, String> {
        let value = value.trim();
        if let Some(hex) = value.strip_prefix('#') {
            return match hex.len() {
                2 => {
                    let gray = u8::from_str_radix(hex, 16)
                        .map_err(|_| format!("invalid grayscale color {value:?}"))?;
                    Ok(Self::new(gray, gray, gray))
                }
                6 => Ok(Self::new(
                    u8::from_str_radix(&hex[0..2], 16)
                        .map_err(|_| format!("invalid hex color {value:?}"))?,
                    u8::from_str_radix(&hex[2..4], 16)
                        .map_err(|_| format!("invalid hex color {value:?}"))?,
                    u8::from_str_radix(&hex[4..6], 16)
                        .map_err(|_| format!("invalid hex color {value:?}"))?,
                )),
                _ => Err(format!("expected #RRGGBB or #BW, got {value:?}")),
            };
        }

        let channels: Vec<&str> = value.split_whitespace().collect();
        if channels.len() != 3 {
            return Err(format!(
                "expected #RRGGBB, #BW, or three RGB channels, got {value:?}"
            ));
        }
        let channel = |index: usize| {
            channels[index]
                .parse::<u8>()
                .map_err(|_| format!("invalid RGB channel in {value:?}"))
        };
        Ok(Self::new(channel(0)?, channel(1)?, channel(2)?))
    }

    fn interpolate(self, other: Self, amount: f64) -> Self {
        let blend = |start: u8, end: u8| {
            (start as f64 + (end as f64 - start as f64) * amount)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        Self::new(
            blend(self.red, other.red),
            blend(self.green, other.green),
            blend(self.blue, other.blue),
        )
    }

    const fn color(self) -> Color {
        Color::Rgb(self.red, self.green, self.blue)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gradient {
    colors: [Color; GRADIENT_STEPS],
}

impl Gradient {
    fn from_stops(start: Rgb, middle: Option<Rgb>, end: Option<Rgb>) -> Self {
        let mut colors = [start.color(); GRADIENT_STEPS];
        let Some(end) = end else {
            return Self { colors };
        };

        for (index, color) in colors.iter_mut().enumerate() {
            let rgb = if let Some(middle) = middle {
                if index <= 50 {
                    start.interpolate(middle, index as f64 / 50.0)
                } else {
                    middle.interpolate(end, (index - 50) as f64 / 50.0)
                }
            } else {
                start.interpolate(end, index as f64 / 100.0)
            };
            *color = rgb.color();
        }
        Self { colors }
    }

    fn stepped(start: Color, middle: Option<Color>, end: Color) -> Self {
        let mut colors = [start; GRADIENT_STEPS];
        for (index, color) in colors.iter_mut().enumerate() {
            *color = if let Some(middle) = middle {
                if index < 34 {
                    start
                } else if index < 67 {
                    middle
                } else {
                    end
                }
            } else if index < 50 {
                start
            } else {
                end
            };
        }
        Self { colors }
    }

    pub fn at(&self, percent: f64) -> Color {
        let index = percent.round().clamp(0.0, 100.0) as usize;
        self.colors[index]
    }

    pub fn at_index(&self, index: usize) -> Color {
        self.colors[index.min(100)]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ThemeColors {
    pub title: Color,
    pub accent: Color,
    pub prompt: Color,
    pub predict: Color,
    pub gpu: Color,
    pub power: Color,
    pub memory: Color,
    pub text: Color,
    pub dim: Color,
    pub bright: Color,
    pub error: Color,
    pub ok: Color,
    pub warn: Color,
    pub border: Color,
    pub border_highlight: Color,
    pub track: Color,
    pub surface: Color,
    pub selected_bg: Color,
    pub selected_fg: Color,
}

#[derive(Debug, Clone)]
pub struct Theme {
    name: String,
    selection: String,
    pub background: Option<Color>,
    pub foreground: Color,
    pub title: Color,
    pub highlight: Color,
    pub selected_bg: Color,
    pub selected_fg: Color,
    pub inactive: Color,
    pub graph_text: Color,
    pub meter_bg: Color,
    pub divider: Color,
    pub throughput_border: Color,
    pub gpu_border: Color,
    pub memory_border: Color,
    pub slots_border: Color,
    pub prompt: Gradient,
    pub predict: Gradient,
    pub gpu: Gradient,
    pub power: Gradient,
    pub memory: Gradient,
    pub cache: Gradient,
    pub temperature: Gradient,
    pub process: Gradient,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
}

impl Theme {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn selection(&self) -> &str {
        &self.selection
    }

    pub fn colors(&self) -> ThemeColors {
        ThemeColors {
            title: self.title,
            accent: self.highlight,
            prompt: self.prompt.at(72.0),
            predict: self.predict.at(72.0),
            gpu: self.gpu.at(72.0),
            power: self.power.at(72.0),
            memory: self.memory.at(72.0),
            text: self.foreground,
            dim: self.inactive,
            bright: self.title,
            error: self.error,
            ok: self.success,
            warn: self.warning,
            border: self.divider,
            border_highlight: self.highlight,
            track: self.meter_bg,
            surface: self.background.unwrap_or(Color::Reset),
            selected_bg: self.selected_bg,
            selected_fg: self.selected_fg,
        }
    }

    fn ltop() -> Self {
        let gradient = |start, middle, end| {
            Gradient::from_stops(
                Rgb::parse(start).expect("valid built-in color"),
                Some(Rgb::parse(middle).expect("valid built-in color")),
                Some(Rgb::parse(end).expect("valid built-in color")),
            )
        };
        Self {
            name: "ltop".to_string(),
            selection: "ltop".to_string(),
            background: Some(Color::Rgb(15, 18, 27)),
            foreground: Color::Rgb(190, 198, 214),
            title: Color::Rgb(235, 240, 248),
            highlight: Color::Rgb(105, 202, 255),
            selected_bg: Color::Rgb(105, 202, 255),
            selected_fg: Color::Rgb(15, 18, 27),
            inactive: Color::Rgb(102, 112, 132),
            graph_text: Color::Rgb(102, 112, 132),
            meter_bg: Color::Rgb(55, 61, 76),
            divider: Color::Rgb(55, 64, 82),
            throughput_border: Color::Rgb(53, 79, 111),
            gpu_border: Color::Rgb(105, 86, 45),
            memory_border: Color::Rgb(91, 60, 116),
            slots_border: Color::Rgb(48, 82, 101),
            prompt: gradient("#315f98", "#69b0ff", "#b4dcff"),
            predict: gradient("#27664a", "#69db9a", "#b7f5ce"),
            gpu: gradient("#7c5c1c", "#f5c45c", "#ffe4a0"),
            power: gradient("#71363b", "#f4787a", "#ffb0b2"),
            memory: gradient("#603985", "#c889ff", "#ead0ff"),
            cache: gradient("#315f72", "#69caff", "#b7ecff"),
            temperature: gradient("#69b0ff", "#f5c45c", "#ff6978"),
            process: gradient("#69db9a", "#f5c45c", "#f4787a"),
            success: Color::Rgb(105, 219, 154),
            warning: Color::Rgb(245, 196, 92),
            error: Color::Rgb(255, 105, 120),
        }
    }

    fn tty() -> Self {
        let cpu = Gradient::stepped(Color::LightGreen, Some(Color::LightYellow), Color::LightRed);
        let temperature = Gradient::stepped(
            Color::LightBlue,
            Some(Color::LightCyan),
            Color::LightMagenta,
        );
        let used = Gradient::stepped(Color::Red, None, Color::LightRed);
        let cached = Gradient::stepped(Color::Cyan, None, Color::LightCyan);
        Self {
            name: "TTY".to_string(),
            selection: "TTY".to_string(),
            background: Some(Color::Black),
            foreground: Color::Gray,
            title: Color::White,
            highlight: Color::LightRed,
            selected_bg: Color::Red,
            selected_fg: Color::White,
            inactive: Color::DarkGray,
            graph_text: Color::DarkGray,
            meter_bg: Color::DarkGray,
            divider: Color::DarkGray,
            throughput_border: Color::Magenta,
            gpu_border: Color::Green,
            memory_border: Color::Yellow,
            slots_border: Color::Red,
            prompt: Gradient::stepped(Color::Blue, None, Color::LightBlue),
            predict: Gradient::stepped(Color::Magenta, None, Color::LightMagenta),
            gpu: cpu.clone(),
            power: cached.clone(),
            memory: used.clone(),
            cache: cached,
            temperature,
            process: cpu,
            success: Color::LightGreen,
            warning: Color::LightYellow,
            error: Color::LightRed,
        }
    }

    fn from_btop(name: &str, selection: String, values: &RawTheme) -> Self {
        let foreground = required_color(values, "main_fg");
        let title = required_color(values, "title");
        let highlight = required_color(values, "hi_fg");
        let inactive = required_color(values, "inactive_fg");
        let cpu = resolved_gradient(values, "cpu");
        let cached = resolved_gradient(values, "cached");
        let used = resolved_gradient(values, "used");
        let process = if values.contains_key("process_start") {
            resolved_gradient(values, "process")
        } else {
            cpu.clone()
        };
        let free = resolved_gradient(values, "free");
        let available = resolved_gradient(values, "available");
        let temperature = resolved_gradient(values, "temp");
        let download = resolved_gradient(values, "download");
        let upload = resolved_gradient(values, "upload");
        let (success, warning, error) = semantic_status_colors(
            &[
                &cpu,
                &cached,
                &used,
                &process,
                &free,
                &available,
                &temperature,
                &download,
                &upload,
            ],
            &[
                highlight.color(),
                required_color(values, "proc_misc").color(),
                required_color(values, "cpu_box").color(),
                required_color(values, "mem_box").color(),
                required_color(values, "net_box").color(),
                required_color(values, "proc_box").color(),
            ],
        );

        Self {
            name: name.to_string(),
            selection,
            background: optional_color(values, "main_bg").map(Rgb::color),
            foreground: foreground.color(),
            title: title.color(),
            highlight: highlight.color(),
            selected_bg: required_color(values, "selected_bg").color(),
            selected_fg: required_color(values, "selected_fg").color(),
            inactive: inactive.color(),
            graph_text: values
                .get("graph_text")
                .copied()
                .flatten()
                .unwrap_or(inactive)
                .color(),
            meter_bg: values
                .get("meter_bg")
                .copied()
                .flatten()
                .unwrap_or(inactive)
                .color(),
            divider: required_color(values, "div_line").color(),
            throughput_border: required_color(values, "net_box").color(),
            gpu_border: required_color(values, "cpu_box").color(),
            memory_border: required_color(values, "mem_box").color(),
            slots_border: required_color(values, "proc_box").color(),
            prompt: download,
            predict: upload,
            gpu: cpu,
            power: cached.clone(),
            memory: used.clone(),
            cache: cached,
            temperature,
            process,
            success,
            warning,
            error,
        }
    }
}

fn semantic_status_colors(gradients: &[&Gradient], accents: &[Color]) -> (Color, Color, Color) {
    let mut candidates = accents.to_vec();
    for gradient in gradients {
        candidates.extend([
            gradient.at_index(0),
            gradient.at_index(50),
            gradient.at_index(100),
        ]);
    }

    (
        nearest_color(Color::Rgb(105, 219, 154), &candidates),
        nearest_color(Color::Rgb(245, 196, 92), &candidates),
        nearest_color(Color::Rgb(255, 105, 120), &candidates),
    )
}

fn nearest_color(target: Color, candidates: &[Color]) -> Color {
    candidates
        .iter()
        .copied()
        .filter(|candidate| matches!(candidate, Color::Rgb(_, _, _)))
        .min_by_key(|candidate| color_distance(target, *candidate))
        .unwrap_or(target)
}

fn color_distance(left: Color, right: Color) -> u32 {
    let (Color::Rgb(left_r, left_g, left_b), Color::Rgb(right_r, right_g, right_b)) = (left, right)
    else {
        return u32::MAX;
    };
    let distance = |left: u8, right: u8| (i32::from(left) - i32::from(right)).unsigned_abs();
    let red = distance(left_r, right_r);
    let green = distance(left_g, right_g);
    let blue = distance(left_b, right_b);
    red * red + green * green + blue * blue
}

type RawTheme = HashMap<String, Option<Rgb>>;

fn parse_btop_theme(name: &str, selection: String, text: &str) -> Result<Theme, String> {
    let mut values = RawTheme::new();
    for (line_number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(rest) = line.strip_prefix("theme[") else {
            continue;
        };
        let Some(end_name) = rest.find(']') else {
            continue;
        };
        let key = &rest[..end_name];
        if !known_key(key) {
            continue;
        }
        let Some((_, raw_value)) = rest[end_name + 1..].split_once('=') else {
            continue;
        };
        let raw_value = raw_value.trim();
        let value = if let Some(quoted) = raw_value.strip_prefix('"') {
            quoted
                .split_once('"')
                .map(|(value, _)| value)
                .unwrap_or(quoted)
        } else {
            raw_value
        }
        .trim();

        if value.is_empty() {
            values.insert(key.to_string(), None);
        } else {
            let color = Rgb::parse(value)
                .map_err(|error| format!("{name}: line {}: {error}", line_number + 1))?;
            values.insert(key.to_string(), Some(color));
        }
    }

    if values.is_empty() {
        return Err(format!("{name}: no recognized btop theme values"));
    }
    Ok(Theme::from_btop(name, selection, &values))
}

fn known_key(key: &str) -> bool {
    matches!(
        key,
        "main_bg"
            | "main_fg"
            | "title"
            | "hi_fg"
            | "selected_bg"
            | "selected_fg"
            | "inactive_fg"
            | "graph_text"
            | "meter_bg"
            | "proc_misc"
            | "cpu_box"
            | "mem_box"
            | "net_box"
            | "proc_box"
            | "div_line"
            | "temp_start"
            | "temp_mid"
            | "temp_end"
            | "cpu_start"
            | "cpu_mid"
            | "cpu_end"
            | "free_start"
            | "free_mid"
            | "free_end"
            | "cached_start"
            | "cached_mid"
            | "cached_end"
            | "available_start"
            | "available_mid"
            | "available_end"
            | "used_start"
            | "used_mid"
            | "used_end"
            | "download_start"
            | "download_mid"
            | "download_end"
            | "upload_start"
            | "upload_mid"
            | "upload_end"
            | "process_start"
            | "process_mid"
            | "process_end"
    )
}

fn required_color(values: &RawTheme, key: &str) -> Rgb {
    values
        .get(key)
        .copied()
        .flatten()
        .or_else(|| default_color(key))
        .expect("all required btop colors have defaults")
}

fn optional_color(values: &RawTheme, key: &str) -> Option<Rgb> {
    match values.get(key) {
        Some(value) => *value,
        None => default_color(key),
    }
}

fn resolved_gradient(values: &RawTheme, name: &str) -> Gradient {
    let start = required_color(values, &format!("{name}_start"));
    let middle = optional_color(values, &format!("{name}_mid"));
    let end = optional_color(values, &format!("{name}_end"));
    Gradient::from_stops(start, middle, end)
}

fn default_color(key: &str) -> Option<Rgb> {
    let value = match key {
        "main_bg" => "#00",
        "main_fg" => "#cc",
        "title" => "#ee",
        "hi_fg" => "#b54040",
        "selected_bg" => "#6a2f2f",
        "selected_fg" => "#ee",
        "inactive_fg" | "graph_text" | "meter_bg" => "#40",
        "proc_misc" => "#0de756",
        "cpu_box" => "#556d59",
        "mem_box" => "#6c6c4b",
        "net_box" => "#5c588d",
        "proc_box" => "#805252",
        "div_line" => "#30",
        "temp_start" => "#4897d4",
        "temp_mid" => "#5474e8",
        "temp_end" => "#ff40b6",
        "cpu_start" => "#77ca9b",
        "cpu_mid" => "#cbc06c",
        "cpu_end" => "#dc4c4c",
        "free_start" => "#384f21",
        "free_mid" => "#b5e685",
        "free_end" => "#dcff85",
        "cached_start" => "#163350",
        "cached_mid" => "#74e6fc",
        "cached_end" => "#26c5ff",
        "available_start" => "#4e3f0e",
        "available_mid" => "#ffd77a",
        "available_end" => "#ffb814",
        "used_start" => "#592b26",
        "used_mid" => "#d9626d",
        "used_end" => "#ff4769",
        "download_start" => "#291f75",
        "download_mid" => "#4f43a3",
        "download_end" => "#b0a9de",
        "upload_start" => "#620665",
        "upload_mid" => "#7d4180",
        "upload_end" => "#dcafde",
        "process_start" => "#80d0a3",
        "process_mid" => "#dcd179",
        "process_end" => "#d45454",
        _ => return None,
    };
    Some(Rgb::parse(value).expect("valid btop default color"))
}

#[derive(Debug, Clone)]
pub struct ThemeCatalog {
    themes: Vec<Theme>,
    warnings: Vec<String>,
}

impl ThemeCatalog {
    pub fn builtin_only() -> Self {
        let mut catalog = Self {
            themes: Vec::new(),
            warnings: Vec::new(),
        };
        catalog.insert_if_missing(Theme::ltop());
        catalog.insert_if_missing(Theme::from_btop(
            "Default",
            "Default".to_string(),
            &RawTheme::new(),
        ));
        catalog.insert_if_missing(Theme::tty());
        for (name, text) in BUNDLED_THEMES {
            match parse_btop_theme(name, canonical_name(name), text) {
                Ok(theme) => catalog.insert_if_missing(theme),
                Err(error) => catalog.warnings.push(error),
            }
        }
        catalog.sort_themes();
        catalog
    }

    pub fn discover(custom_dir: Option<&Path>) -> Self {
        let mut catalog = Self {
            themes: Vec::new(),
            warnings: Vec::new(),
        };
        catalog.insert_if_missing(Theme::ltop());
        catalog.insert_if_missing(Theme::from_btop(
            "Default",
            "Default".to_string(),
            &RawTheme::new(),
        ));
        catalog.insert_if_missing(Theme::tty());

        let config_home = config_home();
        if let Some(path) = custom_dir {
            catalog.add_directory(path);
        }
        if let Some(home) = config_home.as_ref() {
            catalog.add_directory(&home.join("ltop/themes"));
        }
        for (name, text) in BUNDLED_THEMES {
            match parse_btop_theme(name, canonical_name(name), text) {
                Ok(theme) => catalog.insert_if_missing(theme),
                Err(error) => catalog.warnings.push(error),
            }
        }

        for directory in packaged_theme_dirs() {
            catalog.add_directory(&directory);
        }
        if let Some(home) = config_home.as_ref() {
            catalog.add_directory(&home.join("btop/themes"));
        }
        catalog.add_directory(Path::new("/usr/local/share/btop/themes"));
        catalog.add_directory(Path::new("/usr/share/btop/themes"));
        catalog.sort_themes();
        catalog
    }

    pub fn len(&self) -> usize {
        self.themes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.themes.is_empty()
    }

    pub fn theme(&self, index: usize) -> &Theme {
        &self.themes[index.min(self.themes.len().saturating_sub(1))]
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.themes.iter().map(|theme| theme.name())
    }

    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    pub fn index_of(&self, selection: &str) -> Option<usize> {
        let canonical = canonical_name(selection);
        self.themes.iter().position(|theme| {
            canonical_name(theme.name()) == canonical
                || canonical_name(theme.selection()) == canonical
        })
    }

    pub fn resolve(&mut self, selection: &str) -> Result<usize, String> {
        let path = Path::new(selection);
        if path.is_file() {
            let text = fs::read_to_string(path)
                .map_err(|error| format!("could not read theme {}: {error}", path.display()))?;
            let name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("custom")
                .to_string();
            let theme = parse_btop_theme(&name, path.display().to_string(), &text)?;
            if let Some(index) = self.index_of(&name) {
                self.themes[index] = theme;
                return Ok(index);
            }
            self.themes.push(theme);
            self.sort_themes();
            return self
                .index_of(&name)
                .ok_or_else(|| format!("failed to add theme {name}"));
        }

        self.index_of(selection)
            .ok_or_else(|| format!("theme {selection:?} was not found"))
    }

    fn add_directory(&mut self, directory: &Path) {
        if !directory.is_dir() {
            return;
        }
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) => {
                self.warnings.push(format!(
                    "could not read theme directory {}: {error}",
                    directory.display()
                ));
                return;
            }
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("theme"))
            .collect();
        paths.sort();

        for path in paths {
            let Some(name) = path.file_stem().and_then(|name| name.to_str()) else {
                continue;
            };
            if self.index_of(name).is_some() {
                continue;
            }
            match fs::read_to_string(&path) {
                Ok(text) => match parse_btop_theme(name, canonical_name(name), &text) {
                    Ok(theme) => self.insert_if_missing(theme),
                    Err(error) => self.warnings.push(error),
                },
                Err(error) => self
                    .warnings
                    .push(format!("could not read theme {}: {error}", path.display())),
            }
        }
    }

    fn insert_if_missing(&mut self, theme: Theme) {
        if self.index_of(theme.name()).is_none() {
            self.themes.push(theme);
        }
    }

    fn sort_themes(&mut self) {
        let priority = |name: &str| match canonical_name(name).as_str() {
            "ltop" => 0,
            "default" => 1,
            "tty" => 2,
            _ => 3,
        };
        self.themes.sort_by(|left, right| {
            priority(left.name())
                .cmp(&priority(right.name()))
                .then_with(|| left.name().to_lowercase().cmp(&right.name().to_lowercase()))
        });
    }
}

fn canonical_name(name: &str) -> String {
    let path = Path::new(name);
    let name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(name);
    let mut canonical = String::new();
    let mut previous_was_separator = false;
    for character in name.trim().to_lowercase().chars() {
        if matches!(character, ' ' | '_' | '-') {
            if !canonical.is_empty() && !previous_was_separator {
                canonical.push('-');
            }
            previous_was_separator = true;
        } else {
            canonical.push(character);
            previous_was_separator = false;
        }
    }
    canonical.trim_end_matches('-').to_string()
}

fn config_home() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME")
                .filter(|path| !path.is_empty())
                .map(|home| PathBuf::from(home).join(".config"))
        })
}

fn packaged_theme_dirs() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Ok(executable) = env::current_exe() {
        if let Some(bin_dir) = executable.parent() {
            directories.push(bin_dir.join("../share/ltop/themes"));
        }
    }
    directories.push(PathBuf::from("/usr/local/share/ltop/themes"));
    directories.push(PathBuf::from("/usr/share/ltop/themes"));

    let mut seen = HashSet::new();
    directories.retain(|directory| seen.insert(directory.clone()));
    directories
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemePreferences {
    pub theme: String,
    pub theme_background: bool,
    pub update_ms: u64,
}

impl Default for ThemePreferences {
    fn default() -> Self {
        Self {
            theme: DEFAULT_THEME.to_string(),
            theme_background: true,
            update_ms: DEFAULT_UPDATE_MS,
        }
    }
}

impl ThemePreferences {
    pub fn load() -> Result<Self, String> {
        let Some(path) = preferences_path() else {
            return Ok(Self::default());
        };
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => {
                return Err(format!(
                    "could not read preferences {}: {error}",
                    path.display()
                ))
            }
        };
        parse_preferences(&text)
    }

    pub fn save(&self) -> Result<(), String> {
        let Some(path) = preferences_path() else {
            return Err("neither XDG_CONFIG_HOME nor HOME is set".to_string());
        };
        let Some(directory) = path.parent() else {
            return Err("invalid preferences path".to_string());
        };
        fs::create_dir_all(directory).map_err(|error| {
            format!(
                "could not create preferences directory {}: {error}",
                directory.display()
            )
        })?;
        let escaped_theme = self.theme.replace('\\', "\\\\").replace('"', "\\\"");
        let contents = format!(
            "# ltop preferences\ntheme = \"{escaped_theme}\"\ntheme_background = {}\nupdate_ms = {}\n",
            self.theme_background, self.update_ms
        );
        let temporary = path.with_extension("tmp");
        fs::write(&temporary, contents).map_err(|error| {
            format!(
                "could not write preferences {}: {error}",
                temporary.display()
            )
        })?;
        fs::rename(&temporary, &path)
            .map_err(|error| format!("could not replace preferences {}: {error}", path.display()))
    }
}

fn preferences_path() -> Option<PathBuf> {
    config_home().map(|home| home.join("ltop/config"))
}

fn parse_preferences(text: &str) -> Result<ThemePreferences, String> {
    let mut preferences = ThemePreferences::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"');
        match key {
            "theme" if !value.is_empty() => preferences.theme = value.to_string(),
            "theme_background" => preferences.theme_background = value.eq_ignore_ascii_case("true"),
            "update_ms" => preferences.update_ms = parse_update_ms(value)?,
            _ => {}
        }
    }
    Ok(preferences)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn btop_parser_accepts_hex_grayscale_decimal_and_transparency() {
        let theme = parse_btop_theme(
            "fixture",
            "fixture".to_string(),
            r##"
                theme[main_bg]=""
                theme[main_fg]="#7f"
                theme[hi_fg]="1 2 3"
            "##,
        )
        .unwrap();

        assert_eq!(theme.background, None);
        assert_eq!(theme.foreground, Color::Rgb(127, 127, 127));
        assert_eq!(theme.highlight, Color::Rgb(1, 2, 3));
    }

    #[test]
    fn missing_optional_roles_follow_btop_fallbacks() {
        let theme = parse_btop_theme(
            "old-theme",
            "old-theme".to_string(),
            r##"
                theme[inactive_fg]="#123456"
                theme[cpu_start]="#010203"
                theme[cpu_mid]=""
                theme[cpu_end]="#111213"
            "##,
        )
        .unwrap();

        assert_eq!(theme.graph_text, Color::Rgb(0x12, 0x34, 0x56));
        assert_eq!(theme.meter_bg, Color::Rgb(0x12, 0x34, 0x56));
        assert_eq!(theme.process.at_index(0), theme.gpu.at_index(0));
        assert_eq!(theme.process.at_index(100), theme.gpu.at_index(100));
    }

    #[test]
    fn three_stop_gradients_hit_each_control_point() {
        let gradient = Gradient::from_stops(
            Rgb::new(0, 0, 0),
            Some(Rgb::new(100, 50, 0)),
            Some(Rgb::new(200, 100, 50)),
        );

        assert_eq!(gradient.at_index(0), Color::Rgb(0, 0, 0));
        assert_eq!(gradient.at_index(50), Color::Rgb(100, 50, 0));
        assert_eq!(gradient.at_index(100), Color::Rgb(200, 100, 50));
    }

    #[test]
    fn semantic_statuses_stay_intuitive_when_a_theme_reverses_memory_hues() {
        let theme = parse_btop_theme(
            "Dracula",
            "dracula".to_string(),
            include_str!("../themes/dracula.theme"),
        )
        .unwrap();

        assert_eq!(theme.success, Color::Rgb(0x59, 0xb6, 0x90));
        assert_eq!(theme.warning, Color::Rgb(0xff, 0xb8, 0x6c));
        assert_eq!(theme.error, Color::Rgb(0xff, 0x55, 0x55));
    }

    #[test]
    fn bundled_catalog_keeps_the_current_theme_first() {
        let catalog = ThemeCatalog::builtin_only();
        let names: Vec<&str> = catalog.names().collect();

        assert_eq!(names[0], "ltop");
        assert!(names.contains(&"Tokyo Night"));
        assert!(names.contains(&"Solarized Light"));
    }

    #[test]
    fn theme_names_treat_spaces_underscores_and_hyphens_as_aliases() {
        let catalog = ThemeCatalog::builtin_only();

        assert_eq!(
            catalog.index_of("Tokyo Night"),
            catalog.index_of("tokyo-night")
        );
        assert_eq!(
            catalog.index_of("gruvbox_dark"),
            catalog.index_of("Gruvbox Dark")
        );
    }

    #[test]
    fn custom_directory_palettes_override_bundled_aliases() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            env::temp_dir().join(format!("ltop-theme-test-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("tokyo_night.theme"),
            r##"
                theme[main_bg]="#010203"
                theme[main_fg]="#f0f0f0"
            "##,
        )
        .unwrap();

        let catalog = ThemeCatalog::discover(Some(&directory));
        let index = catalog.index_of("Tokyo Night").unwrap();
        assert_eq!(catalog.theme(index).background, Some(Color::Rgb(1, 2, 3)));

        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn preferences_are_small_and_forward_compatible() {
        let preferences = parse_preferences(
            r#"
                theme = "Nord"
                theme_background = false
                update_ms = 750
                future_setting = true
            "#,
        )
        .unwrap();

        assert_eq!(
            preferences,
            ThemePreferences {
                theme: "Nord".to_string(),
                theme_background: false,
                update_ms: 750,
            }
        );
    }

    #[test]
    fn update_interval_matches_btop_bounds() {
        assert_eq!(parse_update_ms("100").unwrap(), MIN_UPDATE_MS);
        assert_eq!(parse_update_ms("86400000").unwrap(), MAX_UPDATE_MS);
        assert!(parse_update_ms("99").is_err());
        assert!(parse_update_ms("86400001").is_err());
        assert!(parse_update_ms("fast").is_err());
    }
}
