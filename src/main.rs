use image::{ImageBuffer, RgbImage};
use rayon::prelude::*;
use std::env;
use std::time::Instant;

// ── Channel helpers ──────────────────────────────────────────────────────────

#[inline] fn to_8bit2(c: u8) -> u8 { (c << 6) | (c << 4) | (c << 2) | c } // RGB222: 0x3 → 0xFF (shift=6)
#[inline] fn to_8bit3(c: u8) -> u8 { (c << 5) | (c << 2) | (c >> 1) } // RGB333: 0x7 → 0xFF (shift=5)
#[inline] fn to_4bit(c: u8)  -> u8 { c >> 4 }
#[inline] fn to_8bit4(c: u8) -> u8 { c * 17 }           // 4-bit → 8-bit (0xF → 0xFF, shift=4)
#[inline] fn to_8bit5(c: u8) -> u8 { (c << 3) | (c >> 2) } // RGB555: 0x1F → 0xFF (shift=3)

#[inline] fn to_6bit(c: u8)  -> u8 { c >> 2 }
// 6-bit → 8-bit: replicate top 2 bits of the 6-bit value into the vacated low bits
// so that 0x3F → 0xFF and the range is linear (0x20 → 0x82, not 0x80).
// NOTE: AGA palette registers are 8-bit, but the HAM8 modifier is 6-bit.
// We use 6-bit throughout for simplicity; palette-load colours lose ~4/255
// per channel vs real AGA hardware — a deliberate accuracy/simplicity tradeoff.
#[inline] fn to_8bit6(c: u8) -> u8 { (c << 2) | (c >> 4) }

/// Squared Euclidean distance in any channel-range RGB space.
#[inline]
fn dist_sq(a: (u8, u8, u8), b: (u8, u8, u8)) -> u32 {
    let dr = (a.0 as i32 - b.0 as i32).pow(2);
    let dg = (a.1 as i32 - b.1 as i32).pow(2);
    let db = (a.2 as i32 - b.2 as i32).pow(2);
    (dr + dg + db) as u32
}

// ── Mode ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Ham6,
    Ham8,
    ExtraHalfBrite,
    Palette16,
    Palette32,
    Sham,
    Palette256,
    Cga,
    Ega,
    Vga,
    Genesis,
    Tg16,
    Snes,
    Nes,
    GameBoy,
    Hercules,
    C64,
    Spectrum,
    Msx,
    AtariSt,
    Sms,
    Atari2600,
}

impl Mode {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "ham6" | "ham"      => Some(Mode::Ham6),
            "ham8"              => Some(Mode::Ham8),
            "ehb" | "halfbrite" => Some(Mode::ExtraHalfBrite),
            "16" | "16color"    => Some(Mode::Palette16),
            "32" | "32color"    => Some(Mode::Palette32),
            "sham"              => Some(Mode::Sham),
            "256" | "256color"  => Some(Mode::Palette256),
            "cga"               => Some(Mode::Cga),
            "ega"               => Some(Mode::Ega),
            "vga"               => Some(Mode::Vga),
            "genesis" | "md"    => Some(Mode::Genesis),
            "tg16" | "pce"      => Some(Mode::Tg16),
            "snes" | "sfc"      => Some(Mode::Snes),
            "nes" | "famicom"   => Some(Mode::Nes),
            "gameboy" | "gb"    => Some(Mode::GameBoy),
            "hercules" | "hgc"  => Some(Mode::Hercules),
            "c64" | "vic2"              => Some(Mode::C64),
            "spectrum" | "zx"           => Some(Mode::Spectrum),
            "msx" | "tms9918"           => Some(Mode::Msx),
            "st" | "atarist"            => Some(Mode::AtariSt),
            "sms" | "mastersystem"      => Some(Mode::Sms),
            "2600" | "atari2600"        => Some(Mode::Atari2600),
            _ => None,
        }
    }

    fn palette_size(self) -> usize {
        match self {
            Mode::Ham6           => 16,
            Mode::Ham8           => 64,
            Mode::ExtraHalfBrite => 32,
            Mode::Palette16      => 16,
            Mode::Palette32      => 32,
            Mode::Sham           => 16,   // per-scanline; not used for global build
            Mode::Palette256     => 256,
            Mode::Cga            => 4,    // fixed hardware palette
            Mode::Ega            => 16,   // fixed hardware palette
            Mode::Vga            => 256,  // Mode 13h, 6-bit DAC
            Mode::Genesis        => 64,   // 4 palettes × 16
            Mode::Tg16           => 256,  // 16 bg palettes × 16
            Mode::Snes           => 256,  // Mode 7 / full palette, RGB555
            Mode::Nes            => 54,   // fixed 2C02 NTSC hardware palette
            Mode::GameBoy        => 4,    // fixed 4-shade green LCD
            Mode::Hercules       => 2,    // fixed black + phosphor
            Mode::C64            => 16,   // fixed VIC-II palette
            Mode::Spectrum       => 15,   // 8 normal + 7 additional bright (black shared)
            Mode::Msx            => 15,   // fixed TMS9918A hardware palette
            Mode::AtariSt        => 16,   // 16 from RGB333 (512 possible)
            Mode::Sms            => 32,   // 32 of 64 possible RGB222 colors
            Mode::Atari2600      => 128,  // fixed NTSC TIA hardware palette
        }
    }

    fn label(self) -> &'static str {
        match self {
            Mode::Ham6           => "HAM6      — OCS/ECS, 16-reg palette, 4-bit channels, up to 4,096 colors",
            Mode::Ham8           => "HAM8      — AGA,     64-reg palette, 6-bit channels, up to 262,144 colors",
            Mode::ExtraHalfBrite => "EHB       — OCS/ECS, 32 direct + 32 half-brightness = 64 colors",
            Mode::Palette16      => "16-color    indexed (OCS 4 bitplanes)",
            Mode::Palette32      => "32-color    indexed (OCS/ECS 5 bitplanes)",
            Mode::Sham           => "SHAM      — Sliced HAM, per-scanline 16-color palettes, reduced fringing",
            Mode::Palette256     => "256-color   indexed (AGA 8 bitplanes)",
            Mode::Cga            => "CGA       — PC CGA fixed 4-color palette (Cyan/Magenta/White)",
            Mode::Ega            => "EGA       — PC EGA fixed 16-color standard palette",
            Mode::Vga            => "VGA       — PC Mode 13h, 256-color 6-bit DAC (18-bit color, 262,144 possible)",
            Mode::Genesis        => "Genesis   — Sega Genesis VDP, 64 colors, RGB333 (9-bit, 512 possible)",
            Mode::Tg16           => "TG-16     — TurboGrafx-16, 256 colors, RGB333 (9-bit, 512 possible)",
            Mode::Snes           => "SNES      — Super Nintendo, 256 colors, RGB555 (15-bit, 32,768 possible)",
            Mode::Nes            => "NES       — Nintendo 2C02 PPU, fixed 54-color NTSC hardware palette",
            Mode::GameBoy        => "Game Boy  — DMG LCD, fixed 4-shade green phosphor palette",
            Mode::Hercules       => "Hercules  — PC HGC monochrome, black + green phosphor",
            Mode::C64            => "C64       — VIC-II hi-res bitmap, 16 colors, 2 per 8×8 block",
            Mode::Spectrum       => "Spectrum  — ZX Spectrum ULA, 15 colors, ink/paper per 8×8 cell (attribute clash)",
            Mode::Msx            => "MSX       — TMS9918A, 15-color fixed hardware palette",
            Mode::AtariSt        => "Atari ST  — Low-res, 16 colors from RGB333 (512 possible)",
            Mode::Sms            => "SMS       — Sega Master System VDP, 32 colors, RGB222 (64 possible)",
            Mode::Atari2600      => "Atari 2600 — TIA, fixed 128-color NTSC palette",
        }
    }

    /// Authentic hardware display resolution for this mode (non-interlaced).
    fn native_resolution(self) -> (u32, u32) {
        match self {
            Mode::Genesis  => (320, 224),
            Mode::Tg16     => (256, 224),
            Mode::Snes     => (256, 224),
            Mode::Nes      => (256, 240),
            Mode::GameBoy  => (160, 144),
            Mode::Hercules => (720, 348),
            Mode::Spectrum | Mode::Msx | Mode::Sms => (256, 192),
            Mode::Atari2600 => (160, 192),
            _              => (320, 200),   // Amiga + CGA/EGA/VGA/C64/AtariSt
        }
    }

    /// Amiga modes support interlace (doubles vertical lines in --res mode).
    /// All PC and console modes have no interlace option in this tool.
    fn supports_interlace(self) -> bool {
        matches!(self, Mode::Ham6 | Mode::Ham8 | Mode::Sham | Mode::ExtraHalfBrite
                     | Mode::Palette16 | Mode::Palette32 | Mode::Palette256)
    }

    /// Effective display resolution accounting for the --interlace flag.
    fn effective_resolution(self, interlace: bool) -> (u32, u32) {
        let (w, h) = self.native_resolution();
        if interlace && self.supports_interlace() { (w, h * 2) } else { (w, h) }
    }

    /// HAM modes use a hold-register that already acts as greedy nearest-color
    /// selection; layering Floyd-Steinberg on top would fight the encoder.
    fn supports_dither(self) -> bool {
        !matches!(self, Mode::Ham6 | Mode::Ham8 | Mode::Sham)
    }

    /// True when the encode for this mode+dither setting runs multi-threaded.
    /// C64 dithers within independent 8×8 blocks, so it stays parallel even
    /// with --dither (unlike EHB/palette F-S, which is inherently serial).
    fn runs_parallel(self, dither: bool) -> bool {
        !dither || !self.supports_dither() || matches!(self, Mode::C64 | Mode::Spectrum)
    }

    /// Short name used in output filenames for `--all` mode.
    fn slug(self) -> &'static str {
        match self {
            Mode::Ham6           => "ham6",
            Mode::Ham8           => "ham8",
            Mode::ExtraHalfBrite => "ehb",
            Mode::Palette16      => "16color",
            Mode::Palette32      => "32color",
            Mode::Sham           => "sham",
            Mode::Palette256     => "256color",
            Mode::Cga            => "cga",
            Mode::Ega            => "ega",
            Mode::Vga            => "vga",
            Mode::Genesis        => "genesis",
            Mode::Tg16           => "tg16",
            Mode::Snes           => "snes",
            Mode::Nes            => "nes",
            Mode::GameBoy        => "gameboy",
            Mode::Hercules       => "hercules",
            Mode::C64            => "c64",
            Mode::Spectrum       => "spectrum",
            Mode::Msx            => "msx",
            Mode::AtariSt        => "atarist",
            Mode::Sms            => "sms",
            Mode::Atari2600      => "atari2600",
        }
    }

    /// All modes in presentation order — used by `--all` to iterate every mode.
    fn all_modes() -> &'static [Mode] {
        &[Mode::Ham6, Mode::Ham8, Mode::Sham, Mode::ExtraHalfBrite,
          Mode::Palette16, Mode::Palette32, Mode::Palette256,
          Mode::Cga, Mode::Ega, Mode::Vga,
          Mode::Genesis, Mode::Tg16, Mode::Snes,
          Mode::Nes, Mode::GameBoy, Mode::Hercules, Mode::C64,
          Mode::Spectrum, Mode::Msx, Mode::AtariSt, Mode::Sms, Mode::Atari2600]
    }
}

// ── Palette building ─────────────────────────────────────────────────────────

/// Build an n-colour popularity palette quantised at `shift` bits per channel.
/// `shift=4` → 4-bit (OCS), `shift=2` → 6-bit (AGA/HAM8).
/// Works on any &[u8] of packed RGB triples — suitable for whole images or
/// individual scanline slices (used by SHAM).
fn build_palette_from_bytes(raw: &[u8], n: usize, shift: u8) -> Vec<(u8, u8, u8)> {
    let bits       = (8 - shift) as usize;
    let channels   = 1usize << bits;
    let table_size = channels * channels * channels;

    let mut counts = vec![0u32; table_size];

    for chunk in raw.chunks_exact(3) {
        let r = (chunk[0] >> shift) as usize;
        let g = (chunk[1] >> shift) as usize;
        let b = (chunk[2] >> shift) as usize;
        counts[(r << (bits * 2)) | (g << bits) | b] += 1;
    }

    let mut entries: Vec<((u8, u8, u8), u32)> = counts
        .iter()
        .enumerate()
        .filter(|(_, &c)| c > 0)
        .map(|(idx, &count)| ((
            ((idx >> (bits * 2)) & (channels - 1)) as u8,
            ((idx >>  bits     ) & (channels - 1)) as u8,
            ( idx                & (channels - 1)) as u8,
        ), count))
        .collect();

    entries.sort_unstable_by_key(|e| std::cmp::Reverse(e.1));
    let mut palette: Vec<(u8, u8, u8)> =
        entries.into_iter().take(n).map(|(c, _)| c).collect();
    while palette.len() < n { palette.push((0, 0, 0)); }
    palette
}

/// Thin wrapper for callers that already have an RgbImage.
fn build_palette(img: &RgbImage, n: usize, shift: u8) -> Vec<(u8, u8, u8)> {
    build_palette_from_bytes(img.as_raw(), n, shift)
}

// ── Fixed hardware palettes (CGA, EGA) ──────────────────────────────────────
//
// All values are in 4-bit space (0–15). The CGA and EGA standard colors happen
// to sit exactly on the 4-bit grid: 0→0, 5→85, 10→170, 15→255 (each × 17).

/// CGA 4-color graphics mode — Palette 1, low-intensity.
/// The iconic cyan/magenta/white look of early PC gaming.
fn cga_palette() -> Vec<(u8, u8, u8)> {
    vec![
        ( 0,  0,  0),   // Black
        ( 0, 10, 10),   // Cyan    (0x00AAAA)
        (10,  0, 10),   // Magenta (0xAA00AA)
        (10, 10, 10),   // White   (0xAAAAAA)
    ]
}

/// EGA standard 16-color palette.
/// Color 6 is Brown (not dark yellow) — the classic EGA hardware quirk
/// where bit 4 of the intensity register halves the green channel.
fn ega_palette() -> Vec<(u8, u8, u8)> {
    vec![
        ( 0,  0,  0),   //  0 Black
        ( 0,  0, 10),   //  1 Blue
        ( 0, 10,  0),   //  2 Green
        ( 0, 10, 10),   //  3 Cyan
        (10,  0,  0),   //  4 Red
        (10,  0, 10),   //  5 Magenta
        (10,  5,  0),   //  6 Brown  ← hardware quirk: not (10,10,0) dark yellow
        (10, 10, 10),   //  7 Light Gray
        ( 5,  5,  5),   //  8 Dark Gray
        ( 5,  5, 15),   //  9 Bright Blue
        ( 5, 15,  5),   // 10 Bright Green
        ( 5, 15, 15),   // 11 Bright Cyan
        (15,  5,  5),   // 12 Bright Red
        (15,  5, 15),   // 13 Bright Magenta
        (15, 15,  5),   // 14 Yellow
        (15, 15, 15),   // 15 White
    ]
}

// ── Fixed console / PC palettes ─────────────────────────────────────────────

// Sega Genesis / Mega Drive and Atari ST have no fixed-palette function:
// their palettes are built from the image with build_palette at shift=5 (RGB333).

/// NES PPU 2C02 — canonical NTSC hardware palette (54 usable entries).
/// Colors are in full 8-bit RGB space. Palette is fixed — never image-derived.
fn nes_palette() -> Vec<(u8, u8, u8)> {
    vec![
        ( 84, 84, 84), ( 0, 30,116), (  8, 16,144), ( 48,  0,136), // 0x00–0x03
        ( 68,  0,100), ( 92,  0, 48), ( 84,  4,  0), ( 60, 24,  0), // 0x04–0x07
        ( 32, 42,  0), (  8, 58,  0), (  0, 64,  0), (  0, 60,  0), // 0x08–0x0B
        (  0, 50, 60), (  0,  0,  0), (  0,  0,  0), (  0,  0,  0), // 0x0C–0x0F
        (152,150,152), (  8, 76,196), ( 48, 50,236), ( 92, 30,228), // 0x10–0x13
        (136, 20,176), (160, 20,100), (152, 34, 32), (120, 60,  0), // 0x14–0x17
        ( 84, 90,  0), ( 40,114,  0), (  8,124,  0), (  0,118, 40), // 0x18–0x1B
        (  0,102,120), (  0,  0,  0), (  0,  0,  0), (  0,  0,  0), // 0x1C–0x1F
        (236,238,236), ( 76,154,236), (120,124,236), (176, 98,236), // 0x20–0x23
        (228, 84,236), (236, 88,180), (236,106,100), (212,136, 32), // 0x24–0x27
        (160,170,  0), (116,196,  0), ( 76,208, 32), ( 56,204,108), // 0x28–0x2B
        ( 56,180,204), ( 60, 60, 60), (  0,  0,  0), (  0,  0,  0), // 0x2C–0x2F
        (236,238,236), (168,204,236), (188,188,236), (212,178,236), // 0x30–0x33
        (236,174,236), (236,174,212), (236,180,176), (228,196,144), // 0x34–0x37
        (204,210,120), (180,222,120), (168,226,144), (152,226,180), // 0x38–0x3B
        (160,214,228), (160,162,160), (  0,  0,  0), (  0,  0,  0), // 0x3C–0x3F
    ]
}

/// Game Boy DMG — 4 shades of the original green LCD phosphor.
fn gameboy_palette() -> Vec<(u8, u8, u8)> {
    vec![
        (155, 188,  15),  // shade 0 — lightest (off-white green)
        (139, 172,  15),  // shade 1 — light
        ( 48,  98,  48),  // shade 2 — dark
        ( 15,  56,  15),  // shade 3 — darkest (near-black green)
    ]
}

/// Hercules HGC — black + green phosphor (medium phosphor green).
fn hercules_palette() -> Vec<(u8, u8, u8)> {
    vec![
        (  0,   0,   0),  // Black
        (  0, 192,   0),  // Green phosphor
    ]
}

/// C64 VIC-II — fixed 16-color palette (Colodore approximation).
fn c64_palette() -> Vec<(u8, u8, u8)> {
    vec![
        (  0,   0,   0),  //  0 Black
        (255, 255, 255),  //  1 White
        (136,  57,  50),  //  2 Red
        (103, 182, 189),  //  3 Cyan
        (139,  63, 150),  //  4 Purple
        ( 85, 160,  73),  //  5 Green
        ( 64,  49, 141),  //  6 Blue
        (191, 206, 114),  //  7 Yellow
        (139,  84,  41),  //  8 Orange
        ( 87,  66,   0),  //  9 Brown
        (184, 105,  98),  // 10 Light Red
        ( 80,  80,  80),  // 11 Dark Gray
        (120, 120, 120),  // 12 Medium Gray
        (148, 224, 137),  // 13 Light Green
        (120, 105, 196),  // 14 Light Blue
        (159, 159, 159),  // 15 Light Gray
    ]
}

/// ZX Spectrum ULA — 15 unique colors (8 normal + 8 bright, black shared).
/// This palette is used for stats display only; simulate_spectrum hardcodes the
/// two brightness groups directly (they are intrinsic to the hardware constraint).
fn spectrum_palette() -> Vec<(u8, u8, u8)> {
    vec![
        (  0,   0,   0),  // Black       (shared between normal and bright)
        (  0,   0, 215),  // Blue
        (215,   0,   0),  // Red
        (215,   0, 215),  // Magenta
        (  0, 215,   0),  // Green
        (  0, 215, 215),  // Cyan
        (215, 215,   0),  // Yellow
        (215, 215, 215),  // White
        (  0,   0, 255),  // Bright Blue
        (255,   0,   0),  // Bright Red
        (255,   0, 255),  // Bright Magenta
        (  0, 255,   0),  // Bright Green
        (  0, 255, 255),  // Bright Cyan
        (255, 255,   0),  // Bright Yellow
        (255, 255, 255),  // Bright White
    ]
}

/// MSX / TMS9918A — 15-color fixed hardware palette.
/// The TMS9918A chip was used in MSX, ColecoVision, TI-99/4A, and Memotech.
/// Color 0 (transparent) is omitted; only the 15 opaque entries are included.
fn msx_palette() -> Vec<(u8, u8, u8)> {
    vec![
        (  0,   0,   0),  //  1 Black
        ( 62, 184,  73),  //  2 Medium Green
        (116, 208, 125),  //  3 Light Green
        ( 89,  85, 224),  //  4 Dark Blue
        (128, 118, 241),  //  5 Light Blue
        (185,  94,  81),  //  6 Dark Red
        (101, 219, 239),  //  7 Cyan
        (219, 101,  89),  //  8 Medium Red
        (255, 137, 125),  //  9 Light Red
        (204, 195,  94),  // 10 Dark Yellow
        (222, 208, 135),  // 11 Light Yellow
        ( 58, 162,  65),  // 12 Dark Green
        (183, 102, 181),  // 13 Magenta
        (204, 204, 204),  // 14 Gray
        (255, 255, 255),  // 15 White
    ]
}

/// Atari 2600 TIA — canonical NTSC palette, 128 colors (16 hues × 8 luminance levels).
/// Based on the Stella emulator reference NTSC palette.
/// Organized as 16 sequential hue groups of 8 entries each (luminance 0–7).
fn atari2600_palette() -> Vec<(u8, u8, u8)> {
    vec![
        // Hue 0: Grayscale
        (  0,  0,  0),(25, 25, 25),(51, 51, 51),(74, 74, 74),
        (100,100,100),(122,122,122),(149,149,149),(173,173,173),
        // Hue 1: Gold
        ( 41, 18,  0),( 66, 29,  0),( 92, 44,  0),(114, 62,  0),
        (140, 79,  0),(163, 99,  0),(189,116,  0),(212,136,  0),
        // Hue 2: Orange
        ( 66,  6,  0),( 92, 19,  0),(117, 36,  0),(140, 55,  0),
        (166, 74,  0),(189, 94,  0),(215,113,  0),(238,133,  0),
        // Hue 3: Red-Orange
        ( 74,  0,  0),( 99,  0,  0),(125,  0,  0),(148,  4,  0),
        (173, 23,  0),(197, 43,  0),(222, 63,  0),(246, 84,  0),
        // Hue 4: Pink
        ( 66,  0, 17),( 92,  0, 33),(117,  0, 52),(140,  0, 69),
        (166,  0, 87),(189,  0,107),(215,  0,127),(238,  0,147),
        // Hue 5: Purple
        ( 49,  0, 51),( 74,  0, 77),( 99,  0,102),(122,  0,125),
        (148,  0,151),(173,  0,174),(197,  0,200),(222,  0,224),
        // Hue 6: Violet
        ( 17,  0, 69),( 43,  0, 94),( 68,  0,120),( 92,  0,143),
        (117,  0,168),(140,  0,193),(166,  0,219),(189,  0,242),
        // Hue 7: Blue-Purple
        (  0,  0, 84),(  0,  0,109),(  4,  0,135),( 28,  0,158),
        ( 54,  0,184),( 77,  0,208),(102,  0,233),(125,  0,255),
        // Hue 8: Blue
        (  0,  0,109),(  0,  0,135),(  0, 14,160),(  0, 38,184),
        (  0, 63,209),(  0, 86,233),(  0,112,255),(  0,136,255),
        // Hue 9: Light Blue
        (  0, 14,122),(  0, 38,148),(  0, 63,173),(  0, 86,197),
        (  0,112,222),(  0,136,246),(  0,161,255),(  0,184,255),
        // Hue A: Teal
        (  0, 38,109),(  0, 63,135),(  0, 86,160),(  0,110,184),
        (  0,135,209),(  0,159,233),(  0,184,255),(  0,207,255),
        // Hue B: Green-Teal
        (  0, 54, 79),(  0, 78,104),(  0,103,130),(  0,126,153),
        (  0,152,178),(  0,176,202),(  0,201,228),(  0,224,251),
        // Hue C: Green
        (  0, 66, 12),(  0, 91, 37),(  0,116, 63),(  0,140, 86),
        (  0,165,112),(  0,189,136),(  0,214,161),(  0,237,184),
        // Hue D: Yellow-Green
        (  0, 60,  0),(  0, 84,  0),(  0,109,  0),(  4,132,  0),
        ( 30,158,  0),( 54,181,  0),( 79,207,  0),(102,230,  0),
        // Hue E: Yellow
        ( 20, 54,  0),( 44, 79,  0),( 69,104,  0),( 93,127,  0),
        (118,153,  0),(142,176,  0),(167,202,  0),(190,225,  0),
        // Hue F: Yellow-Gold
        ( 41, 46,  0),( 66, 70,  0),( 92, 96,  0),(114,119,  0),
        (140,145,  0),(163,168,  0),(189,194,  0),(212,217,  0),
    ]
}

// ── Nearest-palette helper ───────────────────────────────────────────────────

#[inline]
fn nearest(palette: &[(u8, u8, u8)], target: (u8, u8, u8)) -> ((u8, u8, u8), u32) {
    debug_assert!(!palette.is_empty(), "nearest() called with empty palette");
    let mut best      = palette[0];
    let mut best_dist = dist_sq(palette[0], target);
    for &p in &palette[1..] {
        let d = dist_sq(p, target);
        if d < best_dist { best_dist = d; best = p; }
    }
    (best, best_dist)
}

// ── HAM stats (shared by HAM6 and HAM8) ─────────────────────────────────────

#[derive(Default)]
struct HamStats {
    palette_loads: u64,
    modify_r: u64,
    modify_g: u64,
    modify_b: u64,
}
impl HamStats {
    fn total(&self) -> u64 { self.palette_loads + self.modify_r + self.modify_g + self.modify_b }
    fn merge(self, o: Self) -> Self {
        Self {
            palette_loads: self.palette_loads + o.palette_loads,
            modify_r:      self.modify_r      + o.modify_r,
            modify_g:      self.modify_g      + o.modify_g,
            modify_b:      self.modify_b      + o.modify_b,
        }
    }
}

// ── HAM6 simulation (parallel, no dithering) ────────────────────────────────

fn simulate_ham6(img: &RgbImage, palette: &[(u8, u8, u8)]) -> (RgbImage, HamStats) {
    let (width, height, raw_in) = dims(img);
    let mut raw_out = vec![0u8; width * height * 3];

    let row_stats: Vec<HamStats> = raw_out
        .par_chunks_mut(width * 3)
        .enumerate()
        .map(|(y, out_row)| {
            let mut stats = HamStats::default();
            let mut held  = palette[0];
            for x in 0..width {
                let s      = (y * width + x) * 3;
                let target = (to_4bit(raw_in[s]), to_4bit(raw_in[s+1]), to_4bit(raw_in[s+2]));
                let (mut best, mut bd) = nearest(palette, target);
                let mut kind = 0u8;
                if bd > 0 {
                    let c = (target.0, held.1, held.2); let d = dist_sq(c, target);
                    if d < bd { bd = d; best = c; kind = 1; }
                    let c = (held.0, target.1, held.2); let d = dist_sq(c, target);
                    if d < bd { bd = d; best = c; kind = 2; }
                    let c = (held.0, held.1, target.2); let d = dist_sq(c, target);
                    if d < bd { best = c; kind = 3; }
                }
                match kind { 1 => stats.modify_r+=1, 2 => stats.modify_g+=1, 3 => stats.modify_b+=1, _ => stats.palette_loads+=1 }
                held = best;
                put3(out_row, x, to_8bit4(best.0), to_8bit4(best.1), to_8bit4(best.2));
            }
            stats
        })
        .collect();

    let stats = row_stats.into_iter().fold(HamStats::default(), HamStats::merge);
    (mk_image(img, raw_out), stats)
}

// ── HAM8 simulation (parallel, no dithering) ────────────────────────────────

fn simulate_ham8(img: &RgbImage, palette: &[(u8, u8, u8)]) -> (RgbImage, HamStats) {
    let (width, height, raw_in) = dims(img);
    let mut raw_out = vec![0u8; width * height * 3];

    let row_stats: Vec<HamStats> = raw_out
        .par_chunks_mut(width * 3)
        .enumerate()
        .map(|(y, out_row)| {
            let mut stats = HamStats::default();
            let mut held  = palette[0];
            for x in 0..width {
                let s      = (y * width + x) * 3;
                let target = (to_6bit(raw_in[s]), to_6bit(raw_in[s+1]), to_6bit(raw_in[s+2]));
                let (mut best, mut bd) = nearest(palette, target);
                let mut kind = 0u8;
                if bd > 0 {
                    let c = (target.0, held.1, held.2); let d = dist_sq(c, target);
                    if d < bd { bd = d; best = c; kind = 1; }
                    let c = (held.0, target.1, held.2); let d = dist_sq(c, target);
                    if d < bd { bd = d; best = c; kind = 2; }
                    let c = (held.0, held.1, target.2); let d = dist_sq(c, target);
                    if d < bd { best = c; kind = 3; }
                }
                match kind { 1 => stats.modify_r+=1, 2 => stats.modify_g+=1, 3 => stats.modify_b+=1, _ => stats.palette_loads+=1 }
                held = best;
                put3(out_row, x, to_8bit6(best.0), to_8bit6(best.1), to_8bit6(best.2));
            }
            stats
        })
        .collect();

    let stats = row_stats.into_iter().fold(HamStats::default(), HamStats::merge);
    (mk_image(img, raw_out), stats)
}

// ── SHAM simulation (Sliced HAM — per-scanline palette, parallel) ────────────

/// SHAM: each scanline independently builds its own 16-color palette, then
/// runs the standard HAM6 hold-and-modify encode. Per-row palettes match
/// local colors far better than a global palette, dramatically reducing
/// fringing. Historically achieved on real Amiga hardware via the Copper chip
/// reprogramming the palette registers between scanlines.
fn simulate_sham(img: &RgbImage) -> (RgbImage, HamStats) {
    let (width, height, raw_in) = dims(img);
    let mut raw_out = vec![0u8; width * height * 3];

    let row_stats: Vec<HamStats> = raw_out
        .par_chunks_mut(width * 3)
        .enumerate()
        .map(|(y, out_row)| {
            let row_raw = &raw_in[y * width * 3..(y + 1) * width * 3];
            let palette = build_palette_from_bytes(row_raw, 16, 4);
            let mut stats = HamStats::default();
            let mut held  = palette[0];
            for x in 0..width {
                let s      = x * 3;
                let target = (to_4bit(row_raw[s]), to_4bit(row_raw[s+1]), to_4bit(row_raw[s+2]));
                let (mut best, mut bd) = nearest(&palette, target);
                let mut kind = 0u8;
                if bd > 0 {
                    let c = (target.0, held.1, held.2); let d = dist_sq(c, target);
                    if d < bd { bd = d; best = c; kind = 1; }
                    let c = (held.0, target.1, held.2); let d = dist_sq(c, target);
                    if d < bd { bd = d; best = c; kind = 2; }
                    let c = (held.0, held.1, target.2); let d = dist_sq(c, target);
                    if d < bd { best = c; kind = 3; }
                }
                match kind { 1 => stats.modify_r+=1, 2 => stats.modify_g+=1, 3 => stats.modify_b+=1, _ => stats.palette_loads+=1 }
                held = best;
                put3(out_row, x, to_8bit4(best.0), to_8bit4(best.1), to_8bit4(best.2));
            }
            stats
        })
        .collect();

    let stats = row_stats.into_iter().fold(HamStats::default(), HamStats::merge);
    (mk_image(img, raw_out), stats)
}

// ── EHB stats ────────────────────────────────────────────────────────────────

struct EhbStats { direct: u64, half_brite: u64 }

// ── EHB simulation (parallel, no dithering) ─────────────────────────────────

fn simulate_ehb(img: &RgbImage, palette: &[(u8, u8, u8)]) -> (RgbImage, EhbStats) {
    let half = half_brite(palette);
    let (width, height, raw_in) = dims(img);
    let mut raw_out = vec![0u8; width * height * 3];

    let row_counts: Vec<(u64, u64)> = raw_out
        .par_chunks_mut(width * 3)
        .enumerate()
        .map(|(y, out_row)| {
            let (mut nd, mut nh) = (0u64, 0u64);
            for x in 0..width {
                let s      = (y * width + x) * 3;
                let target = (to_4bit(raw_in[s]), to_4bit(raw_in[s+1]), to_4bit(raw_in[s+2]));
                let (best, is_h) = ehb_nearest(palette, &half, target);
                if is_h { nh += 1; } else { nd += 1; }
                put3(out_row, x, to_8bit4(best.0), to_8bit4(best.1), to_8bit4(best.2));
            }
            (nd, nh)
        })
        .collect();

    let (direct, half_brite) = row_counts.into_iter().fold((0,0), |(a,b),(c,d)| (a+c, b+d));
    (mk_image(img, raw_out), EhbStats { direct, half_brite })
}

// ── Generic palette simulation (parallel, no dithering) ─────────────────────

/// Palette simulation parameterized by `shift` (channel quantization) and
/// `expand` (map quantized value back to 8-bit). Covers 3-bit through 6-bit and
/// fixed 8-bit palettes (shift=0, expand=identity).
fn simulate_palette_n(
    img: &RgbImage,
    palette: &[(u8, u8, u8)],
    shift: u8,
    expand: fn(u8) -> u8,
) -> RgbImage {
    let (width, height, raw_in) = dims(img);
    let mut raw_out = vec![0u8; width * height * 3];
    raw_out
        .par_chunks_mut(width * 3)
        .enumerate()
        .for_each(|(y, out_row)| {
            for x in 0..width {
                let s      = (y * width + x) * 3;
                let target = (raw_in[s] >> shift, raw_in[s+1] >> shift, raw_in[s+2] >> shift);
                let (best, _) = nearest(palette, target);
                put3(out_row, x, expand(best.0), expand(best.1), expand(best.2));
            }
        });
    mk_image(img, raw_out)
}

// ── Floyd-Steinberg dithering ────────────────────────────────────────────────

/// Generic Floyd-Steinberg error-diffusion engine.
///
/// `find_best(r, g, b)` receives the adjusted pixel in 8-bit f32 space and
/// returns the output colour as `[r8, g8, b8]` (u8). It may capture mutable
/// state (e.g. for per-mode stats accumulation).
///
/// Error is computed in 8-bit sRGB space and distributed with the standard
/// 7/16 · 3/16 · 5/16 · 1/16 kernel. A two-row rolling buffer keeps
/// memory usage independent of image height.
fn floyd_steinberg<F>(img: &RgbImage, mut find_best: F) -> RgbImage
where F: FnMut(f32, f32, f32) -> [u8; 3]
{
    let width  = img.width()  as usize;
    let height = img.height() as usize;
    let raw_in = img.as_raw();
    let mut raw_out = vec![0u8; width * height * 3];

    // Two-row rolling error buffer, indexed by (row_parity * width + x)
    let mut err = vec![[0.0f32; 3]; width * 2];

    for y in 0..height {
        let cur = (y & 1) * width;         // current row's error base
        let nxt = ((y + 1) & 1) * width;   // next row's error base

        for x in 0..width {
            // Consume accumulated error for this pixel
            let [er, eg, eb] = err[cur + x];
            err[cur + x] = [0.0; 3];

            // Add error to input and clamp to [0, 255]
            let s = (y * width + x) * 3;
            let r = (raw_in[s]   as f32 + er).clamp(0.0, 255.0);
            let g = (raw_in[s+1] as f32 + eg).clamp(0.0, 255.0);
            let b = (raw_in[s+2] as f32 + eb).clamp(0.0, 255.0);

            // Ask the mode-specific colour selector for the best output colour
            let out = find_best(r, g, b);

            // Quantisation error in 8-bit space
            let e = [r - out[0] as f32, g - out[1] as f32, b - out[2] as f32];

            // Distribute error (Floyd-Steinberg kernel):
            //         [  *  7 ]
            //   [ 3   5   1  ] / 16
            if x + 1 < width {
                for c in 0..3 { err[cur + x + 1][c] += e[c] * (7.0 / 16.0); }
            }
            if y + 1 < height {
                if x > 0 {
                    for c in 0..3 { err[nxt + x - 1][c] += e[c] * (3.0 / 16.0); }
                }
                for c in 0..3 { err[nxt + x][c] += e[c] * (5.0 / 16.0); }
                if x + 1 < width {
                    for c in 0..3 { err[nxt + x + 1][c] += e[c] * (1.0 / 16.0); }
                }
            }

            let d = (y * width + x) * 3;
            raw_out[d] = out[0]; raw_out[d+1] = out[1]; raw_out[d+2] = out[2];
        }
    }

    mk_image(img, raw_out)
}

/// Dithered palette simulation — serial Floyd-Steinberg, any bit depth.
fn simulate_palette_n_dither(
    img: &RgbImage,
    palette: &[(u8, u8, u8)],
    shift: u8,
    expand: fn(u8) -> u8,
) -> RgbImage {
    floyd_steinberg(img, |r, g, b| {
        let target = (r as u8 >> shift, g as u8 >> shift, b as u8 >> shift);
        let (best, _) = nearest(palette, target);
        [expand(best.0), expand(best.1), expand(best.2)]
    })
}

/// Dithered EHB simulation (serial Floyd-Steinberg, 64 candidates).
///
/// Stats are accumulated via mutable closure capture. This is safe because
/// `floyd_steinberg` is intentionally single-threaded (F-S error propagation
/// is inherently serial). If `floyd_steinberg` were ever parallelised, the
/// capture would need to become atomic or move to a per-row accumulation.
fn simulate_ehb_dither(img: &RgbImage, palette: &[(u8, u8, u8)]) -> (RgbImage, EhbStats) {
    let half = half_brite(palette);
    let mut n_direct = 0u64;
    let mut n_half   = 0u64;

    let out = floyd_steinberg(img, |r, g, b| {
        let target = (to_4bit(r as u8), to_4bit(g as u8), to_4bit(b as u8));
        let (best, is_h) = ehb_nearest(palette, &half, target);
        if is_h { n_half += 1; } else { n_direct += 1; }
        [to_8bit4(best.0), to_8bit4(best.1), to_8bit4(best.2)]
    });

    (out, EhbStats { direct: n_direct, half_brite: n_half })
}


// ── C64 VIC-II hi-res bitmap simulation ─────────────────────────────────────
//
// VIC-II hi-res mode: each 8×8 pixel block may use exactly 2 colors from the
// 16-color VIC-II palette. The encoder:
//   1. For each 8×8 block, find the 2 palette colors most frequently closest
//      to the pixels in that block.
//   2. Re-map every pixel in the block to whichever of those 2 colors is nearest.
//
// Blocks are independent → trivially parallelizable with rayon.

fn simulate_c64(img: &RgbImage, palette: &[(u8, u8, u8)]) -> RgbImage {
    let (width, height, raw_in) = dims(img);
    let mut raw_out = vec![0u8; width * height * 3];

    // Partition output into 8×8 blocks and process each independently.
    let bw = width.div_ceil(8);    // number of blocks across
    let bh = height.div_ceil(8);   // number of blocks down

    // Collect flat block indices for rayon.
    let block_results: Vec<(usize, Vec<u8>)> = (0..bw * bh)
        .into_par_iter()
        .map(|bi| {
            let bx = (bi % bw) * 8;
            let by = (bi / bw) * 8;
            let pw = (bx + 8).min(width)  - bx;   // actual pixel width of this block
            let ph = (by + 8).min(height) - by;   // actual pixel height of this block

            // 1. Tally nearest-palette hits in this block.
            let mut hits = vec![0u32; palette.len()];
            for py in 0..ph {
                for px in 0..pw {
                    let s = ((by + py) * width + (bx + px)) * 3;
                    let t = (raw_in[s], raw_in[s+1], raw_in[s+2]);
                    let (_, idx) = palette.iter().enumerate()
                        .map(|(i, &p)| (dist_sq(p, t), i))
                        .min_by_key(|&(d, _)| d)
                        .unwrap();
                    hits[idx] += 1;
                }
            }

            // 2. Pick the top-2 palette entries by hit count.
            let mut sorted: Vec<(u32, usize)> =
                hits.iter().enumerate().map(|(i, &h)| (h, i)).collect();
            sorted.sort_unstable_by_key(|e| std::cmp::Reverse(e.0));
            let c0 = palette[sorted[0].1];
            let c1 = palette[sorted[1].1];
            let two = [c0, c1];

            // 3. Re-map every pixel in the block to the closer of the two colors.
            let mut block_out = vec![0u8; pw * ph * 3];
            for py in 0..ph {
                for px in 0..pw {
                    let s = ((by + py) * width + (bx + px)) * 3;
                    let t = (raw_in[s], raw_in[s+1], raw_in[s+2]);
                    let (best, _) = nearest(&two, t);
                    let d = (py * pw + px) * 3;
                    block_out[d] = best.0; block_out[d+1] = best.1; block_out[d+2] = best.2;
                }
            }
            (bi, block_out)
        })
        .collect();

    // Write block results back into the flat output buffer.
    for (bi, block_out) in block_results {
        let bx = (bi % bw) * 8;
        let by = (bi / bw) * 8;
        let pw = (bx + 8).min(width) - bx;
        let ph = (by + 8).min(height) - by;
        for py in 0..ph {
            for px in 0..pw {
                let s = ((by + py) * width + (bx + px)) * 3;
                let d = (py * pw + px) * 3;
                raw_out[s] = block_out[d]; raw_out[s+1] = block_out[d+1]; raw_out[s+2] = block_out[d+2];
            }
        }
    }
    mk_image(img, raw_out)
}

/// C64 hi-res with Floyd-Steinberg dithering within each 8×8 block.
/// Each block selects its own 2-color palette first (same as above), then
/// applies F-S error diffusion restricted to those 2 colors. Blocks remain
/// independent so this is still parallelizable.
fn simulate_c64_dither(img: &RgbImage, palette: &[(u8, u8, u8)]) -> RgbImage {
    let (width, height, raw_in) = dims(img);
    let mut raw_out = vec![0u8; width * height * 3];

    let bw = width.div_ceil(8);
    let bh = height.div_ceil(8);

    let block_results: Vec<(usize, Vec<u8>)> = (0..bw * bh)
        .into_par_iter()
        .map(|bi| {
            let bx = (bi % bw) * 8;
            let by = (bi / bw) * 8;
            let pw = (bx + 8).min(width)  - bx;
            let ph = (by + 8).min(height) - by;

            // Tally and pick top-2 (same as non-dithered path).
            let mut hits = vec![0u32; palette.len()];
            for py in 0..ph {
                for px in 0..pw {
                    let s = ((by + py) * width + (bx + px)) * 3;
                    let t = (raw_in[s], raw_in[s+1], raw_in[s+2]);
                    let (_, idx) = palette.iter().enumerate()
                        .map(|(i, &p)| (dist_sq(p, t), i))
                        .min_by_key(|&(d, _)| d)
                        .unwrap();
                    hits[idx] += 1;
                }
            }
            let mut sorted: Vec<(u32, usize)> =
                hits.iter().enumerate().map(|(i, &h)| (h, i)).collect();
            sorted.sort_unstable_by_key(|e| std::cmp::Reverse(e.0));
            let c0 = palette[sorted[0].1];
            let c1 = palette[sorted[1].1];
            let two = [c0, c1];

            // F-S dither within the block using the 2-color local palette.
            let mut err = vec![[0.0f32; 3]; pw * 2];
            let mut block_out = vec![0u8; pw * ph * 3];
            for py in 0..ph {
                let cur = (py & 1) * pw;
                let nxt = ((py + 1) & 1) * pw;
                for px in 0..pw {
                    let [er, eg, eb] = err[cur + px];
                    err[cur + px] = [0.0; 3];
                    let s = ((by + py) * width + (bx + px)) * 3;
                    let r = (raw_in[s]   as f32 + er).clamp(0.0, 255.0);
                    let g = (raw_in[s+1] as f32 + eg).clamp(0.0, 255.0);
                    let b = (raw_in[s+2] as f32 + eb).clamp(0.0, 255.0);
                    let t = (r as u8, g as u8, b as u8);
                    let (best, _) = nearest(&two, t);
                    let e = [r - best.0 as f32, g - best.1 as f32, b - best.2 as f32];
                    if px + 1 < pw { for c in 0..3 { err[cur + px + 1][c] += e[c] * (7.0/16.0); } }
                    if py + 1 < ph {
                        if px > 0 { for c in 0..3 { err[nxt + px - 1][c] += e[c] * (3.0/16.0); } }
                        for c in 0..3 { err[nxt + px][c] += e[c] * (5.0/16.0); }
                        if px + 1 < pw { for c in 0..3 { err[nxt + px + 1][c] += e[c] * (1.0/16.0); } }
                    }
                    let d = (py * pw + px) * 3;
                    block_out[d] = best.0; block_out[d+1] = best.1; block_out[d+2] = best.2;
                }
            }
            (bi, block_out)
        })
        .collect();

    for (bi, block_out) in block_results {
        let bx = (bi % bw) * 8;
        let by = (bi / bw) * 8;
        let pw = (bx + 8).min(width) - bx;
        let ph = (by + 8).min(height) - by;
        for py in 0..ph {
            for px in 0..pw {
                let s = ((by + py) * width + (bx + px)) * 3;
                let d = (py * pw + px) * 3;
                raw_out[s] = block_out[d]; raw_out[s+1] = block_out[d+1]; raw_out[s+2] = block_out[d+2];
            }
        }
    }
    mk_image(img, raw_out)
}

// ── ZX Spectrum attribute-clash simulation ────────────────────────────────────
//
// ZX Spectrum ULA: each 8×8 pixel cell has one INK and one PAPER color.
// Both colors must come from the SAME brightness group (normal or bright).
// Black appears in both groups (normal black == bright black = (0,0,0)).
//
// Encoder (parallel across blocks):
//   For each block, test both brightness groups:
//     1. Tally nearest-group-color hits per pixel.
//     2. Pick top-2 from that group.
//     3. Compute total squared error for those 2 colors.
//   Choose the group+pair with lowest total error, then remap all pixels.

const SPECTRUM_NORMAL: [(u8, u8, u8); 8] = [
    (  0,   0,   0),  // Black
    (  0,   0, 215),  // Blue
    (215,   0,   0),  // Red
    (215,   0, 215),  // Magenta
    (  0, 215,   0),  // Green
    (  0, 215, 215),  // Cyan
    (215, 215,   0),  // Yellow
    (215, 215, 215),  // White
];
const SPECTRUM_BRIGHT: [(u8, u8, u8); 8] = [
    (  0,   0,   0),  // Black (same as normal)
    (  0,   0, 255),  // Bright Blue
    (255,   0,   0),  // Bright Red
    (255,   0, 255),  // Bright Magenta
    (  0, 255,   0),  // Bright Green
    (  0, 255, 255),  // Bright Cyan
    (255, 255,   0),  // Bright Yellow
    (255, 255, 255),  // Bright White
];

/// Select the best ink/paper pair for a single 8×8 block across both brightness
/// groups. Returns the winning pair (lowest total squared error over the block).
fn spectrum_best_pair(
    raw_in: &[u8], width: usize,
    bx: usize, by: usize, pw: usize, ph: usize,
) -> [(u8, u8, u8); 2] {
    let groups: [&[(u8, u8, u8)]; 2] = [&SPECTRUM_NORMAL, &SPECTRUM_BRIGHT];
    let mut best_pair = [SPECTRUM_NORMAL[0], SPECTRUM_NORMAL[1]];
    let mut best_error = u64::MAX;

    for group in &groups {
        let mut hits = [0u32; 8];
        for py in 0..ph {
            for px in 0..pw {
                let s = ((by + py) * width + (bx + px)) * 3;
                let t = (raw_in[s], raw_in[s+1], raw_in[s+2]);
                let (_, idx) = group.iter().enumerate()
                    .map(|(i, &p)| (dist_sq(p, t), i))
                    .min_by_key(|&(d, _)| d)
                    .unwrap();
                hits[idx] += 1;
            }
        }
        let mut sorted: Vec<(u32, usize)> = hits.iter().enumerate().map(|(i, &h)| (h, i)).collect();
        sorted.sort_unstable_by_key(|e| std::cmp::Reverse(e.0));
        let c0 = group[sorted[0].1];
        let c1 = group[sorted[1].1];
        let two = [c0, c1];
        let total_err: u64 = (0..ph).flat_map(|py| (0..pw).map(move |px| (py, px)))
            .map(|(py, px)| {
                let s = ((by + py) * width + (bx + px)) * 3;
                let t = (raw_in[s], raw_in[s+1], raw_in[s+2]);
                nearest(&two, t).1 as u64
            })
            .sum();
        if total_err < best_error {
            best_error = total_err;
            best_pair = two;
        }
    }
    best_pair
}

fn simulate_spectrum(img: &RgbImage) -> RgbImage {
    let (width, height, raw_in) = dims(img);
    let mut raw_out = vec![0u8; width * height * 3];
    let bw = width.div_ceil(8);
    let bh = height.div_ceil(8);

    let block_results: Vec<(usize, Vec<u8>)> = (0..bw * bh)
        .into_par_iter()
        .map(|bi| {
            let bx = (bi % bw) * 8;
            let by = (bi / bw) * 8;
            let pw = (bx + 8).min(width)  - bx;
            let ph = (by + 8).min(height) - by;
            let two = spectrum_best_pair(raw_in, width, bx, by, pw, ph);
            let mut block_out = vec![0u8; pw * ph * 3];
            for py in 0..ph {
                for px in 0..pw {
                    let s = ((by + py) * width + (bx + px)) * 3;
                    let t = (raw_in[s], raw_in[s+1], raw_in[s+2]);
                    let (best, _) = nearest(&two, t);
                    let d = (py * pw + px) * 3;
                    block_out[d] = best.0; block_out[d+1] = best.1; block_out[d+2] = best.2;
                }
            }
            (bi, block_out)
        })
        .collect();

    for (bi, block_out) in block_results {
        let bx = (bi % bw) * 8;
        let by = (bi / bw) * 8;
        let pw = (bx + 8).min(width) - bx;
        let ph = (by + 8).min(height) - by;
        for py in 0..ph {
            for px in 0..pw {
                let s = ((by + py) * width + (bx + px)) * 3;
                let d = (py * pw + px) * 3;
                raw_out[s] = block_out[d]; raw_out[s+1] = block_out[d+1]; raw_out[s+2] = block_out[d+2];
            }
        }
    }
    mk_image(img, raw_out)
}

/// ZX Spectrum with Floyd-Steinberg dithering within each 8×8 block.
/// The brightness group + ink/paper pair is selected identically to the non-dithered
/// path, then F-S error diffusion is applied within the block using only those 2 colors.
/// Blocks are independent → parallel with rayon.
fn simulate_spectrum_dither(img: &RgbImage) -> RgbImage {
    let (width, height, raw_in) = dims(img);
    let mut raw_out = vec![0u8; width * height * 3];
    let bw = width.div_ceil(8);
    let bh = height.div_ceil(8);

    let block_results: Vec<(usize, Vec<u8>)> = (0..bw * bh)
        .into_par_iter()
        .map(|bi| {
            let bx = (bi % bw) * 8;
            let by = (bi / bw) * 8;
            let pw = (bx + 8).min(width)  - bx;
            let ph = (by + 8).min(height) - by;
            let two = spectrum_best_pair(raw_in, width, bx, by, pw, ph);

            let mut err = vec![[0.0f32; 3]; pw * 2];
            let mut block_out = vec![0u8; pw * ph * 3];
            for py in 0..ph {
                let cur = (py & 1) * pw;
                let nxt = ((py + 1) & 1) * pw;
                for px in 0..pw {
                    let [er, eg, eb] = err[cur + px];
                    err[cur + px] = [0.0; 3];
                    let s = ((by + py) * width + (bx + px)) * 3;
                    let r = (raw_in[s]   as f32 + er).clamp(0.0, 255.0);
                    let g = (raw_in[s+1] as f32 + eg).clamp(0.0, 255.0);
                    let b = (raw_in[s+2] as f32 + eb).clamp(0.0, 255.0);
                    let t = (r as u8, g as u8, b as u8);
                    let (best, _) = nearest(&two, t);
                    let e = [r - best.0 as f32, g - best.1 as f32, b - best.2 as f32];
                    if px + 1 < pw { for c in 0..3 { err[cur + px + 1][c] += e[c] * (7.0/16.0); } }
                    if py + 1 < ph {
                        if px > 0 { for c in 0..3 { err[nxt + px - 1][c] += e[c] * (3.0/16.0); } }
                        for c in 0..3 { err[nxt + px][c] += e[c] * (5.0/16.0); }
                        if px + 1 < pw { for c in 0..3 { err[nxt + px + 1][c] += e[c] * (1.0/16.0); } }
                    }
                    let d = (py * pw + px) * 3;
                    block_out[d] = best.0; block_out[d+1] = best.1; block_out[d+2] = best.2;
                }
            }
            (bi, block_out)
        })
        .collect();

    for (bi, block_out) in block_results {
        let bx = (bi % bw) * 8;
        let by = (bi / bw) * 8;
        let pw = (bx + 8).min(width) - bx;
        let ph = (by + 8).min(height) - by;
        for py in 0..ph {
            for px in 0..pw {
                let s = ((by + py) * width + (bx + px)) * 3;
                let d = (py * pw + px) * 3;
                raw_out[s] = block_out[d]; raw_out[s+1] = block_out[d+1]; raw_out[s+2] = block_out[d+2];
            }
        }
    }
    mk_image(img, raw_out)
}

// ── Simulation result ────────────────────────────────────────────────────────

enum SimResult {
    Ham(HamStats),
    Ehb(EhbStats),
    Palette,
}

// ── Small utilities ──────────────────────────────────────────────────────────

fn dims(img: &RgbImage) -> (usize, usize, &[u8]) {
    (img.width() as usize, img.height() as usize, img.as_raw())
}

fn mk_image(src: &RgbImage, raw: Vec<u8>) -> RgbImage {
    ImageBuffer::from_raw(src.width(), src.height(), raw).unwrap()
}

#[inline]
fn put3(row: &mut [u8], x: usize, r: u8, g: u8, b: u8) {
    let d = x * 3;
    row[d] = r; row[d+1] = g; row[d+2] = b;
}

/// Generate the 32 half-brightness variants of a 32-entry EHB palette.
fn half_brite(palette: &[(u8, u8, u8)]) -> Vec<(u8, u8, u8)> {
    palette.iter().map(|&(r, g, b)| (r >> 1, g >> 1, b >> 1)).collect()
}

/// Find the nearest colour across 32 direct + 32 half-brite EHB candidates.
#[inline]
fn ehb_nearest(
    palette: &[(u8, u8, u8)],
    half: &[(u8, u8, u8)],
    target: (u8, u8, u8),
) -> ((u8, u8, u8), bool) {
    let (mut best, mut best_dist) = nearest(palette, target);
    let mut is_half = false;
    for &h in half {
        let d = dist_sq(h, target);
        if d < best_dist { best_dist = d; best = h; is_half = true; }
    }
    (best, is_half)
}

// ── Shared encode helpers ────────────────────────────────────────────────────

/// Palette dispatch — the big mode/dither match used by both single-mode
/// and --all. Caller is responsible for building `palette` with correct timing.
fn dispatch_encode(img: &RgbImage, palette: &[(u8, u8, u8)], mode: Mode, dither: bool) -> (RgbImage, SimResult) {
    match (mode, dither) {
        (Mode::Ham6,           _    ) => { let (i,s) = simulate_ham6(img, palette);          (i, SimResult::Ham(s))     }
        (Mode::Ham8,           _    ) => { let (i,s) = simulate_ham8(img, palette);          (i, SimResult::Ham(s))     }
        (Mode::Sham,           _    ) => { let (i,s) = simulate_sham(img);                   (i, SimResult::Ham(s))     }
        (Mode::ExtraHalfBrite, true ) => { let (i,s) = simulate_ehb_dither(img, palette);    (i, SimResult::Ehb(s))     }
        (Mode::ExtraHalfBrite, false) => { let (i,s) = simulate_ehb(img, palette);           (i, SimResult::Ehb(s))     }
        // 4-bit: Amiga palette modes + fixed PC palettes (CGA/EGA in 4-bit space)
        (Mode::Palette16 |
         Mode::Palette32 |
         Mode::Palette256 |
         Mode::Cga | Mode::Ega, true ) => (simulate_palette_n_dither(img, palette, 4, to_8bit4), SimResult::Palette),
        (Mode::Palette16 |
         Mode::Palette32 |
         Mode::Palette256 |
         Mode::Cga | Mode::Ega, false) => (simulate_palette_n(img, palette, 4, to_8bit4),        SimResult::Palette),
        // 6-bit: VGA Mode 13h
        (Mode::Vga,             true ) => (simulate_palette_n_dither(img, palette, 2, to_8bit6), SimResult::Palette),
        (Mode::Vga,             false) => (simulate_palette_n(img, palette, 2, to_8bit6),        SimResult::Palette),
        // 3-bit: Genesis, TG-16 (RGB333)
        (Mode::Genesis |
         Mode::Tg16,            true ) => (simulate_palette_n_dither(img, palette, 5, to_8bit3), SimResult::Palette),
        (Mode::Genesis |
         Mode::Tg16,            false) => (simulate_palette_n(img, palette, 5, to_8bit3),        SimResult::Palette),
        // 5-bit: SNES (RGB555)
        (Mode::Snes,            true ) => (simulate_palette_n_dither(img, palette, 3, to_8bit5), SimResult::Palette),
        (Mode::Snes,            false) => (simulate_palette_n(img, palette, 3, to_8bit5),        SimResult::Palette),
        // Fixed 8-bit palettes: NES, Game Boy, Hercules (shift=0, identity expand)
        (Mode::Nes |
         Mode::GameBoy |
         Mode::Hercules,        true ) => (simulate_palette_n_dither(img, palette, 0, |c| c),    SimResult::Palette),
        (Mode::Nes |
         Mode::GameBoy |
         Mode::Hercules,        false) => (simulate_palette_n(img, palette, 0, |c| c),           SimResult::Palette),
        // C64 VIC-II hi-res bitmap: 2 colors per 8×8 block
        (Mode::C64,             true ) => (simulate_c64_dither(img, palette),                    SimResult::Palette),
        (Mode::C64,             false) => (simulate_c64(img, palette),                           SimResult::Palette),
        // Atari ST low-res: 16 colors from RGB333 (identical pipeline to Genesis)
        (Mode::AtariSt,         true ) => (simulate_palette_n_dither(img, palette, 5, to_8bit3), SimResult::Palette),
        (Mode::AtariSt,         false) => (simulate_palette_n(img, palette, 5, to_8bit3),        SimResult::Palette),
        // Sega Master System: 32 colors from RGB222 (2-bit per channel)
        (Mode::Sms,             true ) => (simulate_palette_n_dither(img, palette, 6, to_8bit2), SimResult::Palette),
        (Mode::Sms,             false) => (simulate_palette_n(img, palette, 6, to_8bit2),        SimResult::Palette),
        // MSX/TMS9918 + Atari 2600: fixed 8-bit palettes, nearest-match
        (Mode::Msx |
         Mode::Atari2600,       true ) => (simulate_palette_n_dither(img, palette, 0, |c| c),    SimResult::Palette),
        (Mode::Msx |
         Mode::Atari2600,       false) => (simulate_palette_n(img, palette, 0, |c| c),           SimResult::Palette),
        // ZX Spectrum: attribute-clash block encoder (brightness-group constrained)
        (Mode::Spectrum,        true ) => (simulate_spectrum_dither(img),                        SimResult::Palette),
        (Mode::Spectrum,        false) => (simulate_spectrum(img),                               SimResult::Palette),
    }
}

/// Palette source for a mode — the single authority used by both the
/// single-mode path and `--all`. Fixed hardware palettes are returned as-is;
/// SHAM gets an empty vec (palettes are built per-scanline inside
/// simulate_sham); everything else builds a popularity palette from the image
/// at the mode's quantization depth.
fn mode_palette(img: &RgbImage, mode: Mode) -> Vec<(u8, u8, u8)> {
    match mode {
        Mode::Sham     => vec![],
        Mode::Cga      => cga_palette(),
        Mode::Ega      => ega_palette(),
        Mode::Nes      => nes_palette(),
        Mode::GameBoy  => gameboy_palette(),
        Mode::Hercules => hercules_palette(),
        Mode::C64      => c64_palette(),
        Mode::Spectrum => spectrum_palette(),   // display only; simulation hardcodes groups
        Mode::Msx      => msx_palette(),
        Mode::Atari2600 => atari2600_palette(),
        _ => {
            let shift = match mode {
                Mode::Ham8 | Mode::Vga                     => 2,
                Mode::Snes                                 => 3,
                Mode::Genesis | Mode::Tg16 | Mode::AtariSt => 5,
                Mode::Sms                                  => 6,
                _                                          => 4,
            };
            build_palette(img, mode.palette_size(), shift)
        }
    }
}

/// True when `mode_palette` builds its palette from the image (worth timing
/// and reporting) rather than returning a fixed hardware palette.
fn builds_palette_from_image(mode: Mode) -> bool {
    !matches!(mode,
        Mode::Sham | Mode::Cga | Mode::Ega | Mode::Nes | Mode::GameBoy |
        Mode::Hercules | Mode::C64 | Mode::Spectrum | Mode::Msx | Mode::Atari2600)
}

/// Build palette + encode for one mode — no timing overhead. Used by `--all`.
fn encode_mode(img: &RgbImage, mode: Mode, dither: bool) -> (RgbImage, SimResult) {
    let palette = mode_palette(img, mode);
    dispatch_encode(img, &palette, mode, dither)
}

/// Nearest-neighbor upscale to 1920×1080, centered on a black canvas.
fn upscale_to_1080p(img: RgbImage) -> RgbImage {
    let (fw, fh) = (1920u32, 1080u32);
    let scale = (fw as f32 / img.width()  as f32)
        .min(fh as f32 / img.height() as f32);
    // Round (same as the downscale path) for consistent off-by-zero behaviour,
    // then clamp to (1..=fw/fh) — rounding up at the extreme would overflow the canvas.
    let sw = ((img.width()  as f32 * scale).round() as u32).clamp(1, fw);
    let sh = ((img.height() as f32 * scale).round() as u32).clamp(1, fh);
    let scaled = image::imageops::resize(&img, sw, sh, image::imageops::FilterType::Nearest);
    let mut canvas = RgbImage::new(fw, fh);   // zero-init → black
    let x = ((fw - sw) / 2) as i64;
    let y = ((fh - sh) / 2) as i64;
    image::imageops::overlay(&mut canvas, &scaled, x, y);
    canvas
}

/// Lanczos3 downscale to fit within `max_w × max_h`, preserving aspect ratio.
/// Never upscales — if the image already fits, it's returned at its original size.
fn downscale_to(img: &RgbImage, max_w: u32, max_h: u32) -> RgbImage {
    let scale = (max_w as f32 / img.width()  as f32)
        .min(max_h as f32 / img.height() as f32)
        .min(1.0);
    let sw = ((img.width()  as f32 * scale).round() as u32).max(1);
    let sh = ((img.height() as f32 * scale).round() as u32).max(1);
    image::imageops::resize(img, sw, sh, image::imageops::FilterType::Lanczos3)
}

/// `"photo.jpeg"` + `"ham6"` → `"photo_ham6.png"` (directory is preserved).
fn derive_output_path(input: &str, slug: &str) -> std::path::PathBuf {
    let p    = std::path::Path::new(input);
    let stem = p.file_stem().unwrap_or_default().to_string_lossy();
    let dir  = p.parent().unwrap_or(std::path::Path::new(""));
    dir.join(format!("{}_{}.png", stem, slug))
}

// ── Formatting helpers ───────────────────────────────────────────────────────

fn fmt_count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 { out.push(','); }
        out.push(c);
    }
    out.chars().rev().collect()
}

fn pct(part: u64, total: u64) -> f64 {
    if total == 0 { 0.0 } else { part as f64 / total as f64 * 100.0 }
}

fn print_usage(prog: &str) {
    eprintln!("Usage: {} <mode> [--dither] [--res] [--interlace] <input.(jpg|png)> <output.png>", prog);
    eprintln!("       {} --all  [--dither] [--res] [--interlace] <input.(jpg|png)>", prog);
    eprintln!();
    eprintln!("Modes:");
    eprintln!("  ham6      OCS/ECS HAM6 — 16-reg palette, 4-bit channels, up to 4,096 colors");
    eprintln!("  ham8      AGA     HAM8 — 64-reg palette, 6-bit channels, up to 262,144 colors");
    eprintln!("  sham      OCS/ECS Sliced HAM — per-scanline 16-color palettes, reduced fringing");
    eprintln!("  ehb       OCS/ECS Extra Half-Brite — 32 direct + 32 half-brightness");
    eprintln!("  16color   OCS     16-color indexed palette (4 bitplanes)");
    eprintln!("  32color   OCS/ECS 32-color indexed palette (5 bitplanes)");
    eprintln!("  256color  AGA     256-color indexed palette (8 bitplanes)");
    eprintln!("  cga       PC CGA      fixed 4-color palette (Cyan/Magenta/White)");
    eprintln!("  ega       PC EGA      fixed 16-color standard palette");
    eprintln!("  vga       PC VGA      Mode 13h, 256-color 6-bit DAC palette");
    eprintln!("  hercules  PC HGC      monochrome, black + green phosphor");
    eprintln!("  genesis   Sega MD     64 colors, RGB333, 320×224");
    eprintln!("  tg16      TG-16       256 colors, RGB333, 256×224");
    eprintln!("  snes      Super NES   256 colors, RGB555, 256×224");
    eprintln!("  nes       NES         fixed 54-color 2C02 NTSC hardware palette, 256×240");
    eprintln!("  gameboy   Game Boy    fixed 4-shade green LCD palette, 160×144");
    eprintln!("  c64       C64 VIC-II  fixed 16-color palette, 2 colors per 8×8 block");
    eprintln!("  spectrum  ZX Spectrum fixed 15-color palette, ink/paper per 8×8 cell, 256×192");
    eprintln!("  msx       MSX/TMS9918 fixed 15-color TMS9918A palette, 256×192");
    eprintln!("  atarist   Atari ST    16 colors from RGB333, 320×200");
    eprintln!("  sms       Sega SMS    32 colors from RGB222, 256×192");
    eprintln!("  atari2600 Atari 2600  fixed 128-color NTSC TIA palette, 160×192");
    eprintln!();
    eprintln!("  --dither     Floyd-Steinberg error diffusion (applies to all non-HAM modes)");
    eprintln!("  --res        Downscale to each mode's native resolution, upscale output to 1920×1080");
    eprintln!("  --interlace  Amiga interlaced: doubles vertical lines (requires --res; no effect on PC or console modes)");
    eprintln!("  --all        Run every mode; output to <stem>_<mode>.png alongside input");
}

// ── Entry point ──────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = env::args().collect();

    // Parse flags and positional arguments.
    // Unknown flags are rejected explicitly — silently eating them would drop
    // filenames that start with '-' and give a confusing "need 3 args" error.
    let mut dither     = false;
    let mut res        = false;
    let mut all        = false;
    let mut interlace  = false;
    let mut positional: Vec<&str> = Vec::new();

    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "--dither"    => dither    = true,
            "--res"       => res       = true,
            "--all"       => all       = true,
            "--interlace" => interlace = true,
            s if s.starts_with('-') => {
                eprintln!("Error: unknown flag '{}'.", s);
                print_usage(&args[0]);
                std::process::exit(1);
            }
            s => positional.push(s),
        }
    }

    // ── --all: batch-encode every mode ──────────────────────────────────────────
    if all {
        if positional.len() != 1 {
            eprintln!("Error: --all takes exactly one argument: <input file>.");
            print_usage(&args[0]);
            std::process::exit(1);
        }
        let input_path = positional[0];
        let t_all = Instant::now();

        let img = image::open(input_path).unwrap_or_else(|e| {
            eprintln!("Error: could not open '{}': {}", input_path, e);
            std::process::exit(1);
        }).to_rgb8();

        let (orig_w, orig_h) = (img.width(), img.height());
        eprintln!("Input:  {} ({}×{})", input_path, orig_w, orig_h);
        if res {
            eprintln!("Res:    modes encoded at native resolution → upscaled to 1920×1080");
            if interlace { eprintln!("        (Amiga interlaced: vertical lines doubled)"); }
        }
        if interlace && !res { eprintln!("Note:   --interlace has no effect without --res"); }
        if dither { eprintln!("Dither: Floyd-Steinberg (applicable modes)"); }
        eprintln!();

        // Pre-compute one downscaled image per unique native resolution so modes
        // that share a resolution (e.g. TG-16 and SNES at 256×224) don't repeat work.
        let mut res_cache: std::collections::HashMap<(u32, u32), RgbImage> =
            std::collections::HashMap::new();
        if res {
            for &mode in Mode::all_modes() {
                let key = mode.effective_resolution(interlace);
                res_cache.entry(key).or_insert_with(|| downscale_to(&img, key.0, key.1));
            }
        }

        let mut failures = 0u32;
        for &mode in Mode::all_modes() {
            let work_img: &RgbImage = if res {
                &res_cache[&mode.effective_resolution(interlace)]
            } else {
                &img
            };
            let t_mode = Instant::now();
            let (out, _) = encode_mode(work_img, mode, dither);
            let out      = if res { upscale_to_1080p(out) } else { out };
            let out_path = derive_output_path(input_path, mode.slug());
            let status   = match out.save(&out_path) {
                Ok(())   => "ok",
                Err(e)   => { failures += 1; eprintln!("  Error saving {:?}: {}", out_path, e); "FAILED" }
            };
            eprintln!("  {:10}  →  {}  ({:.0} ms) {}",
                mode.slug(), out_path.display(),
                t_mode.elapsed().as_secs_f64() * 1000.0,
                if status == "ok" { "" } else { status });
        }

        eprintln!();
        eprintln!("Done.  ({:.0} ms total)", t_all.elapsed().as_secs_f64() * 1000.0);
        if failures > 0 {
            eprintln!("Warning: {} mode(s) failed to save.", failures);
            std::process::exit(1);
        }
        return;
    }

    // ── Single-mode path ─────────────────────────────────────────────────────────
    if positional.len() != 3 {
        print_usage(&args[0]);
        std::process::exit(1);
    }

    let mode = Mode::from_str(positional[0]).unwrap_or_else(|| {
        eprintln!("Unknown mode '{}'.", positional[0]);
        print_usage(&args[0]);
        std::process::exit(1);
    });

    let input_path  = positional[1];
    let output_path = positional[2];

    if dither && !mode.supports_dither() {
        eprintln!("Note: --dither has no effect on HAM modes (hold-register encoding is already greedy nearest-color).");
    }
    if interlace && !mode.supports_interlace() {
        eprintln!("Note: --interlace only affects Amiga modes; has no effect on PC or console modes.");
    }
    if interlace && !res {
        eprintln!("Note: --interlace has no effect without --res.");
    }

    let t_total = Instant::now();

    let img = image::open(input_path).unwrap_or_else(|e| {
        eprintln!("Error: could not open '{}': {}", input_path, e);
        std::process::exit(1);
    }).to_rgb8();

    let (orig_w, orig_h) = (img.width(), img.height());
    let total_orig_px    = orig_w as u64 * orig_h as u64;
    eprintln!("Input:    {} ({}×{}, {} px)", input_path, orig_w, orig_h, fmt_count(total_orig_px));
    eprintln!("Mode:     {}", mode.label());
    if res {
        let (nw, nh) = mode.effective_resolution(interlace);
        eprintln!("Native res: {}×{}{} → upscaled to 1920×1080", nw, nh,
            if interlace && mode.supports_interlace() { " (interlaced)" } else { "" });
    }
    if dither && mode.supports_dither() {
        eprintln!("Dither:   Floyd-Steinberg ({})",
            if mode.runs_parallel(true) { "block-parallel" } else { "serial" });
    }

    // --res: downscale the input to the mode's authentic screen resolution before
    // simulation. Lanczos3 preserves detail better than bilinear when shrinking.
    // --interlace doubles vertical resolution (320×200 → 320×400) for Amiga modes.
    let img = if res {
        let (nw, nh) = mode.effective_resolution(interlace);
        downscale_to(&img, nw, nh)
    } else {
        img
    };

    let (w, h)   = (img.width(), img.height());
    let total_px = w as u64 * h as u64;

    // SHAM builds per-scanline palettes inside simulate_sham.
    // Fixed-palette modes skip image analysis entirely.
    let t_pal   = Instant::now();
    let palette = mode_palette(&img, mode);
    let pal_ms  = if builds_palette_from_image(mode) {
        t_pal.elapsed().as_secs_f64() * 1000.0
    } else {
        0.0
    };

    let parallel = mode.runs_parallel(dither);
    if parallel {
        eprintln!("Encoding  ({} threads)…", rayon::current_num_threads());
    } else {
        eprintln!("Encoding  (1 thread, serial dither)…");
    }

    let t_enc = Instant::now();

    let (out_img, sim_result) = dispatch_encode(&img, &palette, mode, dither);

    let enc_ms   = t_enc.elapsed().as_secs_f64() * 1000.0;
    // Guard against div-by-zero on instant encodes (tiny --res images can finish in <1µs).
    let enc_secs = (enc_ms / 1000.0).max(1e-9);

    let out_img = if res { upscale_to_1080p(out_img) } else { out_img };

    let t_save = Instant::now();
    out_img.save(output_path).unwrap_or_else(|e| {
        eprintln!("Error: could not save '{}': {}", output_path, e);
        std::process::exit(1);
    });
    let save_ms  = t_save.elapsed().as_secs_f64() * 1000.0;
    let total_ms = t_total.elapsed().as_secs_f64() * 1000.0;

    // Count unique RGB colors in the saved output image.
    let n_unique_colors: usize = {
        let mut seen = std::collections::HashSet::new();
        for p in out_img.pixels() { seen.insert((p[0], p[1], p[2])); }
        seen.len()
    };

    eprintln!();
    eprintln!("── Stats ────────────────────────────────");
    eprintln!("  Output:        {}", output_path);
    if res {
        eprintln!("  Sim res:       {}×{} ({} px) — encoded at native resolution", w, h, fmt_count(total_px));
        eprintln!("  Output res:    1920×1080 — nearest-neighbor upscale on black canvas");
    } else {
        eprintln!("  Resolution:    {}×{} ({} px)", w, h, fmt_count(total_px));
    }
    eprintln!("  Unique colors: {}", fmt_count(n_unique_colors as u64));
    eprintln!();
    match mode {
        Mode::Sham     => eprintln!("  Palette:       per-scanline ({} rows × 16 colors)", h),
        Mode::Cga      => eprintln!("  Palette:       fixed — CGA hardware (4 colors)"),
        Mode::Ega      => eprintln!("  Palette:       fixed — EGA hardware (16 colors)"),
        Mode::Nes      => eprintln!("  Palette:       fixed — NES 2C02 NTSC hardware (54 colors)"),
        Mode::GameBoy  => eprintln!("  Palette:       fixed — Game Boy DMG green LCD (4 shades)"),
        Mode::Hercules => eprintln!("  Palette:       fixed — Hercules HGC phosphor (2 colors)"),
        Mode::C64      => eprintln!("  Palette:       fixed — C64 VIC-II (16 colors, 2 per 8×8 block)"),
        Mode::Spectrum => eprintln!("  Palette:       fixed — ZX Spectrum ULA (15 colors, ink/paper per 8×8 block)"),
        Mode::Msx      => eprintln!("  Palette:       fixed — MSX TMS9918A hardware (15 colors)"),
        Mode::Atari2600 => eprintln!("  Palette:       fixed — Atari 2600 TIA NTSC hardware (128 colors)"),
        _              => eprintln!("  Palette build: {:.1} ms", pal_ms),
    }
    eprintln!("  Encode:        {:.1} ms  ({:.0} lines/s, {:.2} Mpx/s)",
        enc_ms, h as f64 / enc_secs, total_px as f64 / enc_secs / 1e6);
    eprintln!("  PNG save:      {:.1} ms", save_ms);
    eprintln!("  Total:         {:.1} ms", total_ms);

    match sim_result {
        SimResult::Ham(s) => {
            let total = s.total();
            let mods  = s.modify_r + s.modify_g + s.modify_b;
            eprintln!();
            eprintln!("  Encoding breakdown ({} px):", fmt_count(total));
            eprintln!("    Palette load:  {} ({:.1}%)", fmt_count(s.palette_loads), pct(s.palette_loads, total));
            eprintln!("    Modify R:      {} ({:.1}%)", fmt_count(s.modify_r),      pct(s.modify_r,      total));
            eprintln!("    Modify G:      {} ({:.1}%)", fmt_count(s.modify_g),      pct(s.modify_g,      total));
            eprintln!("    Modify B:      {} ({:.1}%)", fmt_count(s.modify_b),      pct(s.modify_b,      total));
            eprintln!("    Modify total:  {} ({:.1}%)", fmt_count(mods),            pct(mods,            total));
        }
        SimResult::Ehb(s) => {
            let total = s.direct + s.half_brite;
            eprintln!();
            eprintln!("  Color usage ({} px):", fmt_count(total));
            eprintln!("    Direct colors:     {} ({:.1}%)", fmt_count(s.direct),     pct(s.direct,     total));
            eprintln!("    Half-brite colors: {} ({:.1}%)", fmt_count(s.half_brite), pct(s.half_brite, total));
        }
        SimResult::Palette => {}
    }

    eprintln!("─────────────────────────────────────────");
}
