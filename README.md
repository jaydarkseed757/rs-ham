# rs-ham

A command-line tool that converts JPG or PNG images into PNGs that visually simulate
vintage graphics hardware — Amiga, classic PC, and retro consoles.

Supports 17 modes across Amiga (HAM6, HAM8, SHAM, EHB, 16/32/256-color), PC (CGA,
EGA, VGA, Hercules), and consoles (Sega Genesis, TurboGrafx-16, Super Nintendo, NES,
Game Boy, C64). Optional Floyd-Steinberg dithering for all palette-based modes.

---

## Build

```bash
cargo build --release
```

The binary lands at `target/release/rs-ham`. Requires Rust 1.70+.

---

## Usage

```
rs-ham <mode>  [--dither] [--res] [--interlace] <input.(jpg|png)> <output.png>
rs-ham --all   [--dither] [--res] [--interlace] <input.(jpg|png)>
```

| Mode | Hardware | Colors |
|---|---|---|
| `ham6` | OCS/ECS | 16-register palette + hold-and-modify → up to 4,096 reachable |
| `ham8` | AGA | 64-register palette + hold-and-modify → up to 262,144 reachable |
| `sham` | OCS/ECS | Sliced HAM: per-scanline 16-color palettes, dramatically reduced fringing |
| `ehb` | OCS/ECS | Extra Half-Brite: 32 direct + 32 auto half-brightness = 64 total |
| `16color` | OCS | 16-color indexed palette (4 bitplanes) |
| `32color` | OCS/ECS | 32-color indexed palette (5 bitplanes) |
| `256color` | AGA | 256-color indexed palette (8 bitplanes) |
| `cga` | PC CGA | Fixed 4-color hardware palette (Cyan/Magenta/White) |
| `ega` | PC EGA | Fixed 16-color hardware palette |
| `vga` | PC VGA | 256-color Mode 13h, 6-bit DAC palette (18-bit color) |
| `hercules` | PC HGC | Monochrome, 2 colors — black + green phosphor, 720×348 |
| `genesis` | Sega Genesis | 64 colors, RGB333 (9-bit), 320×224 |
| `tg16` | TurboGrafx-16 | 256 colors, RGB333 (9-bit), 256×224 |
| `snes` | Super Nintendo | 256 colors, RGB555 (15-bit), 256×224 |
| `nes` | NES | Fixed 54-color 2C02 NTSC hardware palette, 256×240 |
| `gameboy` | Game Boy DMG | Fixed 4-shade green LCD palette, 160×144 |
| `c64` | C64 VIC-II | Fixed 16-color palette, hi-res bitmap: 2 colors per 8×8 block |

`--dither` enables Floyd-Steinberg error diffusion. Applies to all palette-based
modes (`ehb`, `16color`, `32color`, `256color`, `cga`, `ega`). Has no effect on
HAM or SHAM modes (explained below).

`--all` runs every mode on the input and saves each result as `<stem>_<mode>.png`
next to the input file. Combines freely with `--dither`, `--res`, and `--interlace`.

`--res` renders at the mode's authentic hardware resolution — 320×200 for Amiga and
early PC, 320×224 for Genesis, 256×224 for TG-16/SNES, 256×240 for NES, 160×144 for
Game Boy, 720×348 for Hercules — then upscales to 1920×1080 using nearest-neighbor
scaling centered on a black canvas. Produces the characteristic fat-pixel retro look.

`--interlace` simulates Amiga interlaced mode. Requires `--res`. When combined,
the downscale target becomes **320×400** (doubling the vertical line count) instead
of 320×200. The simulation runs on the taller image, then upscales to 1920×1080 as
usual. This approximates how photographers would use interlace to capture finer
vertical detail on the Amiga — at the cost of inter-line flicker on real hardware.
Has no effect on CGA or EGA modes (PC hardware; interlace not modeled here).

### Examples

```bash
# Classic HAM6 with fringing artifacts
rs-ham ham6 photo.jpg ham6_out.png

# Sliced HAM — same hardware, much less fringing
rs-ham sham photo.jpg sham_out.png

# High-quality AGA HAM8
rs-ham ham8 photo.jpg ham8_out.png

# 16-color with dithering for better gradients
rs-ham 16color --dither photo.jpg 16color_dithered.png

# Extra Half-Brite with dithering
rs-ham ehb --dither photo.jpg ehb_dithered.png

# AGA 256-color with dithering
rs-ham 256color --dither photo.jpg 256color_dithered.png

# CGA classic cyan/magenta look
rs-ham cga photo.jpg cga_out.png

# EGA 16-color with dithering
rs-ham ega --dither photo.jpg ega_dithered.png

# VGA Mode 13h — 256 colors, 6-bit DAC
rs-ham vga photo.jpg vga_out.png
rs-ham vga --dither photo.jpg vga_dithered.png
rs-ham vga --res photo.jpg vga_native.png

# Hercules monochrome — black + green phosphor, 720×348
rs-ham hercules --res photo.jpg hercules_native.png

# Sega Genesis — 64 colors, RGB333, 320×224
rs-ham genesis --res photo.jpg genesis_native.png
rs-ham genesis --res --dither photo.jpg genesis_dithered.png

# TurboGrafx-16 — 256 colors, RGB333, 256×224
rs-ham tg16 --res photo.jpg tg16_native.png

# Super Nintendo — 256 colors, RGB555, 256×224
rs-ham snes --res photo.jpg snes_native.png
rs-ham snes --res --dither photo.jpg snes_dithered.png

# NES — fixed 54-color hardware palette, 256×240
rs-ham nes --res photo.jpg nes_native.png

# Game Boy — 4 shades of green, 160×144 (very chunky blocks)
rs-ham gameboy --res photo.jpg gameboy_native.png
rs-ham gameboy --res --dither photo.jpg gameboy_dithered.png

# C64 VIC-II — 16 colors, 2 per 8×8 block
rs-ham c64 --res photo.jpg c64_native.png
rs-ham c64 --res --dither photo.jpg c64_dithered.png

# Any mode at authentic hardware resolution (320×200 → 1920×1080)
rs-ham cga --res photo.jpg cga_native.png
rs-ham sham --res photo.jpg sham_native.png
rs-ham ega --dither --res photo.jpg ega_native_dithered.png

# Amiga interlaced mode (320×400 → 1920×1080) — Amiga modes only
rs-ham ham6 --res --interlace photo.jpg ham6_interlaced.png
rs-ham sham --res --interlace photo.jpg sham_interlaced.png

# All 9 modes in one shot → photo_ham6.png, photo_cga.png, …
rs-ham --all photo.jpg

# All modes at native res with dithering
rs-ham --all --res --dither photo.jpg

# All modes at native res, Amiga modes interlaced (320×400), PC modes 320×200
rs-ham --all --res --interlace photo.jpg
```

---

## How each mode works

### Shared: palette building

Most modes start by building a palette from the image using a **popularity algorithm**:

1. Every pixel is quantized to the mode's channel precision (4-bit for OCS modes,
   6-bit for HAM8).
2. A frequency table counts how many pixels fall in each quantized color bucket.
3. The N most-frequent colors become the palette registers.

This means the palette is optimized for the specific image rather than being a fixed
set of colors.

Two exceptions:
- **SHAM** skips the global step and builds a fresh 16-color palette per scanline.
- **CGA and EGA** use fixed hardware palettes — the colors are hardcoded to match
  the real hardware regardless of image content.

---

### HAM6 — Hold-And-Modify (OCS/ECS)

HAM6 is the most technically interesting mode. It uses 6 bitplanes: 2 control bits
and 4 data bits per pixel. The control bits select one of four operations:

| Control | Operation |
|---|---|
| `00` | Load palette register (4 data bits = register index 0–15) |
| `01` | Modify Blue: replace blue channel with 4 data bits |
| `10` | Modify Red: replace red channel with 4 data bits |
| `11` | Modify Green: replace green channel with 4 data bits |

The key concept is the **hold register** — a 12-bit color (4 bits per channel) that
carries the current pixel's color forward to the next pixel. Modify operations change
only one channel of the hold register; the other two channels are inherited ("held")
from the previous pixel.

This lets the hardware address 4,096 colors (16 × 16 × 16 in 12-bit space) using
only a 16-color palette, at the cost of the fringing artifact.

#### The encoder (greedy, left-to-right)

For each pixel, the encoder evaluates 19 candidates:

- **16 palette registers** — outputs that exact color, no dependency on held state
- **Modify R** — `(target_r, held_g, held_b)`
- **Modify G** — `(held_r, target_g, held_b)`
- **Modify B** — `(held_r, held_g, target_b)`

It picks the candidate with the minimum squared Euclidean distance to the target
color in 4-bit RGB space. The chosen color becomes the new held color for the next
pixel.

The hold register **resets to palette register 0** at the start of each scanline,
matching Amiga hardware behavior.

#### Fringing

When the encoder needs to reach a color that differs from the held color in more than
one channel, it cannot get there in a single step. It takes the closest single-step
approximation, and the next pixel(s) continue converging toward the target. This
produces the **horizontal color smear** — the fringe — visible at sharp color
transitions. It bleeds left-to-right within a scanline but never crosses scanline
boundaries.

Fringing is an authentic hardware artifact. It is not corrected or hidden.

---

### HAM8 — Hold-And-Modify (AGA)

HAM8 is the AGA-era upgrade. Same concept as HAM6, but with 8 bitplanes: 2 control
bits + 6 data bits per pixel.

| Spec | HAM6 | HAM8 |
|---|---|---|
| Hardware | OCS/ECS (A500, A600, A2000…) | AGA (A1200, A4000) |
| Palette registers | 16 | 64 |
| Channel precision | 4-bit (0–15) | 6-bit (0–63) |
| Color space | 12-bit, 4,096 reachable | 18-bit, 262,144 reachable |

With 6-bit channel precision, modify operations can get much closer to the target
color in a single step. Fringing is dramatically reduced — often invisible to the
casual eye. Palette-loaded pixels use 6-bit precision throughout the simulation
(a small simplification vs. real AGA 8-bit registers, amounting to ≤ 4/255 per
channel).

---

### SHAM — Sliced HAM (OCS/ECS)

SHAM is the historically most-used technique for high-quality photo display on the
original Amiga. It runs the same HAM6 algorithm as `ham6`, but instead of building
a single global 16-color palette for the whole image, it builds a **fresh 16-color
palette for each individual scanline**.

On real hardware this was achieved with the **Copper chip**: a co-processor that
could fire DMA events mid-frame to reprogram the color registers between scanlines.
The CPU would prepare the per-row palettes in advance (often using a tool like SHAM
or PhotonPaint), and the Copper would swap them in at the right moment.

#### Why SHAM dramatically reduces fringing

The HAM6 encoder's fringing comes from the hold register having to modify multiple
channels to converge on a target color that the palette can't provide directly. With
a global palette, one palette has to serve every part of the image — warm skin tones
and a cool sky share the same 16 colors.

With a per-scanline palette, each row gets 16 colors optimized specifically for that
row's content. The encoder can usually load an exact or near-exact palette match for
most pixels, leaving fewer multi-channel corrections needed. This is reflected in
the stats:

| Mode | Palette load% | Modify total% |
|---|---|---|
| `ham6` (global) | ~48% | ~52% |
| `sham` (per-row) | ~85% | ~15% |

The trade-off is that scanline boundaries can show a slight horizontal color shift
where the palette changes — authentic to the original technique.

---

### 256-color — AGA indexed palette

Standard indexed-color mode using 256 entries. Same nearest-color logic as
`16color` and `32color`, just with 8 bitplanes instead of 4 or 5. Exclusive to
AGA hardware (A1200, A4000).

With 256 colors, smooth gradients are much more convincing than with 32 or fewer,
especially on portraits and natural scenes. `--dither` further reduces banding for
difficult gradients.

---

### CGA — PC Color Graphics Adapter

CGA was IBM's first color graphics card (1981). Its 4-color graphics mode is one
of the most recognizable aesthetics in computing history.

Unlike the Amiga modes, CGA uses a **fixed hardware palette** — the programmer
couldn't freely choose colors. The card offered two palettes and two intensity
levels; this tool simulates **Palette 1, low intensity**: the iconic combination
of Black, Cyan, Magenta, and White.

| Index | Color | 8-bit RGB |
|---|---|---|
| 0 | Black | `#000000` |
| 1 | Cyan | `#00AAAA` |
| 2 | Magenta | `#AA00AA` |
| 3 | White | `#AAAAAA` |

No palette build step — the four colors are hardcoded to match the CGA hardware.
`--dither` applies Floyd-Steinberg dithering across just these four colors, which
produces surprisingly readable results via the ordered noise pattern.

---

### EGA — PC Enhanced Graphics Adapter

EGA (1984) brought 16 simultaneous colors chosen from a palette of 64. The
standard 16-color palette it shipped with became iconic — and is still the
default color scheme in many terminals and text editors today.

Like CGA, this tool uses the **fixed standard EGA palette** rather than building
one from the image. Notable quirk: Color 6 is **Brown**, not dark yellow. On real
EGA hardware, a special circuit halved the green channel on color register 6,
producing `#AA5500` instead of the `#AAAA00` you'd expect from the bit pattern.

| # | Color | # | Color |
|---|---|---|---|
| 0 | Black | 8 | Dark Gray |
| 1 | Blue | 9 | Bright Blue |
| 2 | Green | 10 | Bright Green |
| 3 | Cyan | 11 | Bright Cyan |
| 4 | Red | 12 | Bright Red |
| 5 | Magenta | 13 | Bright Magenta |
| 6 | **Brown** | 14 | Yellow |
| 7 | Light Gray | 15 | White |

`--dither` applies Floyd-Steinberg dithering across the 16 EGA colors.

---

### VGA — PC Video Graphics Array (Mode 13h)

VGA (1987) introduced Mode 13h: 320×200 with 256 simultaneous colors, each chosen
freely from a **6-bit-per-channel DAC** — an 18-bit color space with 262,144
possible colors. This was a dramatic leap from EGA's fixed 16-color palette.

Unlike CGA and EGA, VGA's palette is not fixed. This tool builds it from the image
using the same popularity algorithm as the Amiga modes, quantized to 6-bit channels
(steps of 4 in 8-bit space: 0, 4, 8, …, 252). The result is 256 colors optimized
for the specific image, which is how real Mode 13h software worked — DOS games and
demos loaded custom palettes tailored to their content.

The 6-bit DAC is identical to HAM8's color space. The difference is that VGA gets
256 arbitrary indexed colors where HAM8 gets 64 palette registers plus hold-and-modify
chaining. VGA trades HAM's color-reaching mechanism for a cleaner, fringe-free lookup.

`--dither` applies Floyd-Steinberg dithering, which is particularly effective at
VGA's 256-color level — gradients look very smooth.

---

### Hercules — PC Hercules Graphics Card

HGC (1982) was the dominant monochrome graphics card before CGA became affordable.
At **720×348** it had more horizontal resolution than CGA's 640 — enough to display
crisp text and technical drawings. The display used a green or amber phosphor monitor;
this tool simulates green phosphor: black `(0,0,0)` and `(0,192,0)`.

Fixed 2-color palette. `--dither` applies Floyd-Steinberg to distribute luminance
error, which produces halftone-like patterns that convey grayscale shading
surprisingly well at 2 colors.

---

### Sega Genesis — Mega Drive VDP

The Genesis VDP (1988) uses a **9-bit color space** (RGB333 — 3 bits per channel,
8 levels each, 512 possible colors). The hardware provides 4 palettes of 16 colors
each = 64 colors simultaneously on screen.

This tool builds the 64-color palette from the image using the popularity algorithm,
quantized to 3-bit channels (steps of 36 in 8-bit space). Native resolution is
**320×224** (the H40 mode used by most Genesis games).

---

### TurboGrafx-16 — PC Engine HuC6270

The TG-16 uses the same **RGB333 9-bit** color space as the Genesis, but with more
palette capacity: 512 total entries organized as 32 palettes of 16 colors. Background
layers can draw from 16 palettes = **256 simultaneous colors**, significantly more
than the Genesis.

This tool builds a 256-color image-derived palette in RGB333 space. Native resolution
is **256×224**.

---

### Super Nintendo — S-PPU

The SNES (1990) uses a **15-bit color space** (RGB555 — 5 bits per channel, 32 levels
each, 32,768 possible colors). This is a dramatic step up from the 9-bit Genesis and
TG-16. Mode 7 and direct-color modes can display all 256 palette entries simultaneously,
each chosen from the full 32,768-color space.

This tool simulates a 256-color palette built from the image, quantized to 5-bit
channels. Native resolution is **256×224**.

The visual difference from VGA (which also does 256 colors): VGA's palette comes from
a 6-bit DAC (262,144 possible colors) while SNES uses 5-bit (32,768). In practice
both look very smooth at 256 colors — the limit is the 256 distinct entries, not the
color space.

---

### NES — 2C02 PPU

The NES PPU has a **fixed hardware palette** — not programmable, baked into the chip's
NTSC signal generation circuitry. The standard palette has 64 entries (0x00–0x3F) of
which 54 are distinct visible colors; the rest are variants of black.

Like CGA and EGA, this tool uses the fixed hardware palette directly. The colors
represent the NTSC analog output of the 2C02 chip; they are not simple RGB values
but the result of composite signal timing. Native resolution is **256×240** (the full
PPU output; real TVs typically cropped ~8 pixels per edge as overscan).

---

### Game Boy — DMG LCD

The original Game Boy (1989) has a 160×144 reflective LCD with 4 levels of
brightness — no true color, just shades. The display had a distinctive green-yellow
phosphor tint and was notoriously slow (causing motion blur on fast-moving sprites,
also known as "smearing").

This tool uses 4 fixed shades matching the original DMG LCD color:

| Shade | 8-bit RGB |
|---|---|
| 0 (lightest) | `(155, 188, 15)` |
| 1 | `(139, 172, 15)` |
| 2 | `(48, 98, 48)` |
| 3 (darkest) | `(15, 56, 15)` |

At `--res`, the native **160×144** is upscaled to 1920×1080, making each hardware
pixel roughly 7×7 screen pixels — the chunkiest output in the tool.

---

### C64 — VIC-II Hi-Res Bitmap

The Commodore 64's VIC-II chip (1982) has a fixed **16-color palette** that became
one of the most recognized color sets in computing. Hi-res bitmap mode runs at
**320×200** with a constraint: each 8×8 pixel block can use at most **2 of the 16
colors** — one background and one foreground, freely chosen per block.

This constraint is what gives C64 graphics their characteristic look: smooth areas
render cleanly, but complex color transitions show visible 8-pixel-wide banding where
the block boundaries force a color choice.

The encoder (for each 8×8 block):
1. Find the 2 VIC-II palette colors most frequently nearest to the pixels in this block.
2. Re-map every pixel in the block to whichever of those 2 colors is closer.

`--dither` applies Floyd-Steinberg *within each block* using its own 2-color local
palette. Blocks are processed in parallel since they are fully independent.

The palette used is the **Colodore** approximation, which is considered one of the
most accurate measurements of the VIC-II's NTSC output.

---

### EHB — Extra Half-Brite

EHB uses 6 bitplanes to address 64 colors: 32 directly programmable palette registers
plus 32 **automatically generated** half-brightness variants.

The hardware generates the half-brite set by shifting each color register's channels
right by 1 bit (integer divide by 2):

```
half_brite[i] = (palette[i].r >> 1, palette[i].g >> 1, palette[i].b >> 1)
```

So a palette color of `(14, 8, 4)` in 4-bit space automatically produces a
half-brite entry of `(7, 4, 2)`. The programmer cannot set these directly — they are
always derived from the direct palette.

The encoder is simpler than HAM: for each pixel, find the closest color among all 64
candidates (32 direct + 32 half-brite). No hold register, no scanline dependency, no
fringing. Half-brite colors are useful for shadows, dark backgrounds, and subtle
gradients beyond what 32 colors alone could achieve.

---

### 16-color, 32-color, and 256-color palette modes

Standard indexed-color modes. Each pixel is mapped to the nearest color in the
palette using squared Euclidean distance in 4-bit RGB space. No hold register, no
modify operations, no fringing.

- **16-color** (4 bitplanes): the standard OCS low-res color mode
- **32-color** (5 bitplanes): adds one bitplane for twice the palette entries
- **256-color** (8 bitplanes): AGA only (A1200, A4000); dramatically more convincing
  gradients on portraits and natural scenes

Without dithering, smooth gradients will show visible banding, especially at low
palette sizes. With `--dither`, Floyd-Steinberg error diffusion spreads quantization
error across neighboring pixels to break up banding at the cost of some noise texture.

---

## Floyd-Steinberg dithering

When `--dither` is passed with a palette or EHB mode, the encoder uses Floyd-Steinberg
error diffusion instead of simple nearest-color mapping.

For each pixel (processed left-to-right, top-to-bottom):
1. Add any accumulated error from neighboring pixels to the raw input color.
2. Clamp to `[0, 255]`.
3. Find the nearest palette color.
4. Compute the quantization error: `adjusted_color − output_color`.
5. Distribute the error to four neighbors using the standard kernel:

```
         [  *   7 ]
  [ 3    5    1  ] / 16
```

where `*` is the current pixel. Error is computed and distributed in 8-bit space
regardless of the mode's native channel precision.

#### Why HAM and SHAM modes don't support dithering

HAM's greedy hold-register encoder already selects the closest reachable color at
each pixel — it is inherently a form of nearest-color quantization. Applying F-S
dithering on top would pre-distort the input in ways that fight the hold-register
logic: the encoder would see the dither noise as real color data and try to track it,
producing worse fringing rather than better gradients. The two approaches work against
each other. SHAM uses the same hold-register logic, so it has the same constraint.

#### Performance note

Floyd-Steinberg is inherently serial (each pixel depends on its left and top-left
neighbors), so `--dither` runs on a single thread. Without `--dither`, all modes use
rayon to process scanlines in parallel.

---

## Native resolution rendering (`--res` and `--interlace`)

All the modes described above simulate the color constraints of the hardware but
operate at whatever resolution the input image happens to be. A 2MP phone photo
encoded as CGA still produces a 2MP output — the color is authentic but the
resolution is not.

`--res` makes the simulation fully authentic end-to-end:

1. **Downscale** the input to the mode's authentic native resolution, maintaining
   aspect ratio. Each platform's native resolution:

   | Platform | Native resolution |
   |---|---|
   | Amiga, CGA, EGA, VGA, C64 | 320×200 |
   | Sega Genesis | 320×224 |
   | TurboGrafx-16, Super Nintendo | 256×224 |
   | NES | 256×240 |
   | Game Boy | 160×144 |
   | Hercules | 720×348 |

2. **Simulate** colors at that tiny resolution — the palette, dithering, and
   HAM/block encoding all run on the downscaled image exactly as the hardware would.
3. **Upscale** to 1920×1080 using nearest-neighbor scaling, centered on a black
   canvas with letterbox/pillarbox bars filling the rest.

The result is a 1920×1080 PNG where each hardware pixel is rendered as a clearly
visible block. Game Boy output at 160×144 produces especially massive pixels;
Hercules at 720×348 produces a wide ultraletterbox with fine horizontal detail.

Adding `--interlace` (Amiga modes only) changes step 1 to target **320×400**
instead of 320×200, doubling the scanline count to simulate the Amiga interlaced
display mode. On real hardware, Amiga interlace interleaved even and odd fields at
50/60 Hz, producing flicker but enabling finer vertical detail — particularly
useful for photographic images displayed with tools like SHAM or HAM8. Step 3
(upscale to 1920×1080) is unchanged. PC and console modes ignore `--interlace`.

All modes support `--res`, and it combines freely with `--dither`. `--interlace`
requires `--res` and has no effect without it.

---

## Stats output

Every run prints a timing and encoding breakdown:

```
── Stats ────────────────────────────────
  Output:        out.png
  Resolution:    1008×1986 (2,001,888 px)

  Palette build: 6.5 ms
  Encode:        11.3 ms  (175,155 lines/s, 176.56 Mpx/s)
  PNG save:      10.8 ms
  Total:         45.6 ms

  Encoding breakdown (2,001,888 px):        ← HAM6/HAM8/SHAM only
    Palette load:  965,500 (48.2%)
    Modify R:      813,552 (40.6%)
    Modify G:      108,561 ( 5.4%)
    Modify B:      114,275 ( 5.7%)
    Modify total: 1,036,388 (51.8%)
─────────────────────────────────────────
```

For SHAM, the palette line reads `per-scanline (N rows × 16 colors)` instead of a
build time, since each row builds its own palette during encoding:

```
  Palette:       per-scanline (1986 rows × 16 colors)
```

The encoding breakdown for HAM/SHAM modes shows what fraction of pixels used a
direct palette load vs. each modify operation. SHAM's per-row palettes produce
~85% palette loads (vs ~48% for global-palette HAM6), since local palettes match
row colors so precisely that chained modify operations are rarely needed.

For EHB modes, the breakdown shows direct vs. half-brite color usage.

---

## Implementation notes

- **Color space**: a single generic `simulate_palette_n(shift, expand)` function covers
  all quantization depths — 3-bit (Genesis/TG-16, shift=5), 4-bit (Amiga/CGA/EGA,
  shift=4), 5-bit (SNES, shift=3), 6-bit (HAM8/VGA, shift=2), and fixed 8-bit palettes
  (NES/Game Boy/Hercules/C64, shift=0 with identity expand). Expand functions use
  bit-replication to fill the low bits: `to_8bit3(c) = (c<<5)|(c<<2)|(c>>1)`,
  `to_8bit4(c) = c*17`, `to_8bit5(c) = (c<<3)|(c>>2)`, `to_8bit6(c) = (c<<2)|(c>>4)`.

- **C64 block encoder**: `simulate_c64` processes each 8×8 block independently —
  tallies nearest-palette hits, selects the top 2 colors, then re-maps all pixels.
  Blocks are embarrassingly parallel; processed via rayon with no shared state.
  The dithered variant runs F-S within each block using its 2-color local palette.

- **Palette algorithm**: popularity (frequency) — the N most common quantized colors
  in the image. Simple and fast; a median-cut or octree approach would produce better
  palettes for images with smooth gradients.

- **No lookahead**: the HAM/SHAM encoder is strictly greedy. A lookahead optimizer
  could reduce fringing by occasionally choosing a suboptimal pixel to set up a
  better trajectory, but this would not be authentic hardware behavior.

- **Parallelism**: non-dithered modes process all scanlines in parallel via
  [rayon](https://github.com/rayon-rs/rayon). HAM and SHAM scanlines are independent
  because the hold register resets at the start of each row; SHAM's per-row palette
  build also happens inside the parallel closure. Dithered modes are serial by
  necessity.

- **Palette building**: uses a `vec![0u32; N]` frequency table (N = 4,096 for 4-bit
  modes, 262,144 for HAM8) rather than a hash map — no heap allocation or hashing in
  the hot path. For SHAM, a fresh 4,096-entry table is allocated per scanline inside
  the rayon closure; each allocation is small (16 KB) and short-lived.

---

## Dependencies

```toml
[dependencies]
image = "0.25"   # JPEG/PNG decode + encode
rayon = "1"      # parallel scanline processing
```
