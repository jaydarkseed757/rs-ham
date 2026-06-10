# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build --release          # optimized build (always use for testing output quality/speed)
cargo build                    # debug build (faster compile, slower encode)
cargo run --release -- <args>  # run directly
cargo clippy                   # lint
```

No tests exist yet. Manual verification pattern:

```bash
cargo run --release -- <mode> [--dither] [--res] [--interlace] <input.jpg> <output.png>
cargo run --release -- --all [--dither] [--res] <input.jpg>   # runs all 22 modes
```

## Architecture

**Single-file project** — all ~1600 lines live in `src/main.rs`. No modules.

### Core pipeline (single-mode path)

1. Parse flags (`--dither`, `--res`, `--interlace`, `--all`) and positional args
2. Optionally downscale input to the mode's native resolution (`downscale_to` via Lanczos3)
3. Build palette from image or use fixed hardware palette (`build_palette` / hardcoded functions)
4. Encode: `dispatch_encode` → mode-specific simulation function
5. Optionally upscale output to 1920×1080 (`upscale_to_1080p`, nearest-neighbor, black canvas)
6. Save PNG, print stats

### Mode system

`Mode` enum has 22 variants. Each mode implements:
- `palette_size()` — how many colors
- `label()` — human-readable description
- `native_resolution()` — authentic hardware resolution (used by `--res`)
- `effective_resolution(interlace)` — native_res × 2 vertical if interlaced
- `supports_interlace()` — **whitelist**: only Amiga modes (Ham6/Ham8/Sham/EHB/Palette16/Palette32/Palette256)
- `supports_dither()` — false only for HAM6/HAM8/Sham (hold-register encoding conflicts with F-S)
- `runs_parallel(dither)` — false for serial F-S modes, true for block-parallel modes (C64, Spectrum)
- `slug()` — used in `--all` output filenames

### Simulation function taxonomy

| Category | Functions | Notes |
|---|---|---|
| HAM (hold-and-modify) | `simulate_ham6`, `simulate_ham8` | Per-row parallel; no dither variant |
| SHAM | `simulate_sham` | Per-scanline palette; no palette param |
| EHB (extra half-brite) | `simulate_ehb`, `simulate_ehb_dither` | 64 candidates = 32 direct + 32 half-brightness |
| Generic palette | `simulate_palette_n(img, palette, shift, expand)` | Covers all bit-depth variants via parameters |
| Generic palette dither | `simulate_palette_n_dither(img, palette, shift, expand)` | Serial F-S via `floyd_steinberg` |
| C64 block | `simulate_c64`, `simulate_c64_dither` | 2 colors per 8×8 block; parallel across blocks |
| Spectrum block | `simulate_spectrum`, `simulate_spectrum_dither` | Tests both brightness groups; picks lower-error pair |

### Quantization depth / channel helpers

The `shift` and `expand` parameters to `simulate_palette_n` fully determine color depth:

| Hardware | Shift | Expand fn | Range |
|---|---|---|---|
| RGB222 (SMS) | 6 | `to_8bit2` | 0–3 |
| RGB333 (Genesis/TG-16/Atari ST) | 5 | `to_8bit3` | 0–7 |
| 4-bit (Amiga OCS/CGA/EGA) | 4 | `to_8bit4` | 0–15 |
| RGB555 (SNES) | 3 | `to_8bit5` | 0–31 |
| 6-bit DAC (VGA/HAM8) | 2 | `to_8bit6` | 0–63 |
| Fixed 8-bit (NES/GameBoy/etc.) | 0 | `\|c\| c` | 0–255 |

Palettes built with `build_palette(img, n, shift)` store values in the quantized space (0..2^(8-shift)). `expand` maps them back to 8-bit for output pixels.

### `--all` mode

Pre-computes a `HashMap<(u32,u32), RgbImage>` of downscaled images keyed by native resolution before iterating modes — so modes sharing a resolution (e.g. Spectrum/MSX/SMS all at 256×192) compute the downscale only once.

### ZX Spectrum constraint

Unlike C64 (any 2 from 16 colors per block), Spectrum's attribute clash requires both ink and paper to come from the **same brightness group** (normal 8 or bright 8; black is shared). `spectrum_best_pair` tests both groups, picks the lower-error pair, and is called by both `simulate_spectrum` and `simulate_spectrum_dither`.

### Fixed hardware palettes

`nes_palette`, `gameboy_palette`, `hercules_palette`, `c64_palette`, `spectrum_palette`, `msx_palette`, `atari2600_palette` — all return `Vec<(u8,u8,u8)>` in full 8-bit space. The Spectrum palette is used for stats display only; the simulation uses the hardcoded `SPECTRUM_NORMAL` / `SPECTRUM_BRIGHT` const arrays instead.
