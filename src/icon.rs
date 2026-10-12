//! Procedurally drawn application icon. Dependency-free so that `build.rs`
//! can include this file (`#[path = "src/icon.rs"]`) to produce the `.ico`
//! embedded in the executable and the raw RGBA the app shows (window icon,
//! start page, About), so that nothing is rasterised at start-up.

// build.rs and the app use different parts.
#![allow(dead_code)]

/// Sizes in the `.ico` of the executable.
pub const ICO_SIZES: [u32; 8] = [16, 20, 24, 32, 40, 48, 64, 256];

/// Straight RGBA, channels 0..1.
type Rgba = [f32; 4];

fn hex(c: u32) -> Rgba {
    let ch = |s: u32| ((c >> s) & 0xff) as f32 / 255.0;
    [ch(16), ch(8), ch(0), 1.0]
}

fn with_alpha(c: Rgba, a: f32) -> Rgba {
    [c[0], c[1], c[2], c[3] * a.clamp(0.0, 1.0)]
}

/// Paint `top` over `under`.
fn over(under: Rgba, top: Rgba) -> Rgba {
    let a = top[3] + under[3] * (1.0 - top[3]);
    if a <= 0.0 {
        return [0.0; 4];
    }
    let mut out = [0.0, 0.0, 0.0, a];
    for i in 0..3 {
        out[i] = (top[i] * top[3] + under[i] * under[3] * (1.0 - top[3])) / a;
    }
    out
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Signed distance from `(x, y)` to a box from `min` to `max` with corners
/// rounded by `r` (negative inside).
fn rounded_box(x: f32, y: f32, min: (f32, f32), max: (f32, f32), r: f32) -> f32 {
    let (cx, cy) = ((min.0 + max.0) / 2.0, (min.1 + max.1) / 2.0);
    let (hx, hy) = ((max.0 - min.0) / 2.0 - r, (max.1 - min.1) / 2.0 - r);
    let qx = (x - cx).abs() - hx;
    let qy = (y - cy).abs() - hy;
    let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
    outside + qx.max(qy).min(0.0) - r
}

fn mix(a: Rgba, b: Rgba, t: f32) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

/// Signed distance to the triangle `a`, `b`, `c` (negative inside).
fn triangle(x: f32, y: f32, a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
    let edge = |p0: (f32, f32), p1: (f32, f32)| {
        let (ex, ey) = (p1.0 - p0.0, p1.1 - p0.1);
        let (wx, wy) = (x - p0.0, y - p0.1);
        let t = ((wx * ex + wy * ey) / (ex * ex + ey * ey)).clamp(0.0, 1.0);
        let (dx, dy) = (wx - ex * t, wy - ey * t);
        (dx * dx + dy * dy, ex * wy - ey * wx)
    };
    let (d0, s0) = edge(a, b);
    let (d1, s1) = edge(b, c);
    let (d2, s2) = edge(c, a);
    let d = d0.min(d1).min(d2).sqrt();
    let inside = (s0 >= 0.0 && s1 >= 0.0 && s2 >= 0.0) || (s0 <= 0.0 && s1 <= 0.0 && s2 <= 0.0);
    if inside { -d } else { d }
}

/// A film frame of graphite with light sprocket holes and a blue
/// screen, flat: soft gradients, a hairline of light along the top, a soft
/// shadow below.
const FRAME: ((f32, f32), (f32, f32)) = ((0.04, 0.12), (0.96, 0.88));
const FRAME_RADIUS: f32 = 0.129;
/// The holes' centres across and down, their size and corner radius.
const HOLE_X: [f32; 2] = [0.151, 0.849];
const HOLE_Y: [f32; 3] = [0.272, 0.5, 0.728];
const HOLE: (f32, f32) = (0.069, 0.076);
const HOLE_RADIUS: f32 = 0.028;
const SCREEN: ((f32, f32), (f32, f32)) = ((0.234, 0.158), (0.766, 0.842));
const SCREEN_RADIUS: f32 = 0.076;
/// Play (before its corners are rounded by `PLAY_ROUND`): centred across
/// the screen, then moved right a little, as its weight is at its left side
/// and exactly centred it looks shifted left.
const PLAY: [(f32, f32); 3] = [(0.390, 0.360), (0.642, 0.5), (0.390, 0.640)];
const PLAY_ROUND: f32 = 0.031;

/// The colour of the icon at `(x, y)`, both in 0..1 of the icon's side, for
/// an icon of `size` pixels: a graphite film frame whose screen is blue with a
/// white Play. Below 25 px the frame has no holes (they would be noise).
fn sample(x: f32, y: f32, size: u32) -> Rgba {
    // Anti-aliasing width in icon units: a pixel at any size (a fixed width
    // in icon units blurs the edges of the large sizes).
    let aa = 0.5 / size as f32;
    let cover = |d: f32| smoothstep(aa, -aa, d);
    let mut c = [0.0; 4];

    // The frame, a soft shadow under it.
    let (min, max) = FRAME;
    let (width, height) = (max.0 - min.0, max.1 - min.1);
    let frame = rounded_box(x, y, min, max, FRAME_RADIUS);
    let below = rounded_box(
        x,
        y,
        (min.0, min.1 + 0.02),
        (max.0, max.1 + 0.02),
        FRAME_RADIUS,
    );
    let shadow = (-below.max(0.0) / 0.02).exp() * smoothstep(0.08, 0.0, below) * 0.35;
    c = over(c, with_alpha(hex(0x000000), shadow * (1.0 - cover(frame))));
    let t = (x - min.0) / width * 0.35 + (y - min.1) / height * 0.65;
    c = over(
        c,
        with_alpha(mix(hex(0x3a3f48), hex(0x16191e), t), cover(frame)),
    );
    let top = (1.0 - 2.0 * (y - min.1) / height).clamp(0.0, 1.0);
    let hairline = (1.0 + frame / 0.008).clamp(0.0, 1.0) * cover(frame) * top;
    c = over(c, with_alpha(hex(0xffffff), hairline * 0.35));

    if size >= 25 {
        let (hw, hh) = (HOLE.0 / 2.0, HOLE.1 / 2.0);
        for hx in HOLE_X {
            for hy in HOLE_Y {
                let hole = rounded_box(x, y, (hx - hw, hy - hh), (hx + hw, hy + hh), HOLE_RADIUS);
                c = over(c, with_alpha(hex(0xe6eaf0), cover(hole) * 0.85));
            }
        }
    }

    // The screen, lighter at its top left.
    let ((sx0, sy0), (sx1, sy1)) = SCREEN;
    let screen = rounded_box(x, y, (sx0, sy0), (sx1, sy1), SCREEN_RADIUS);
    let st = (x - sx0) / (sx1 - sx0) * 0.4 + (y - sy0) / (sy1 - sy0) * 0.6;
    c = over(
        c,
        with_alpha(mix(hex(0x4fa3ff), hex(0x2a4fd8), st), cover(screen)),
    );

    // Play, white, with a soft shadow below it.
    let play = |dy: f32| {
        let [a, b, p] = PLAY.map(|(px, py)| (px, py + dy));
        triangle(x, y, a, b, p) - PLAY_ROUND
    };
    let shade = (-play(0.012).max(0.0) / 0.02).exp() * cover(screen);
    c = over(c, with_alpha(hex(0x0b1f6b), shade * 0.45));
    c = over(c, with_alpha(hex(0xffffff), cover(play(0.0))));
    c
}

/// Straight (non-premultiplied) RGBA pixels, `size * size * 4` bytes, row
/// major from the top. 4x4 supersampling gives smooth edges.
pub fn rgba(size: u32) -> Vec<u8> {
    const SS: u32 = 4;
    let n = size as usize;
    let mut out = vec![0u8; n * n * 4];
    for py in 0..size {
        for px in 0..size {
            // Averaged premultiplied, so transparent samples add no colour.
            let mut acc = [0.0f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / size as f32;
                    let c = sample(x, y, size);
                    for i in 0..3 {
                        acc[i] += c[i] * c[3];
                    }
                    acc[3] += c[3];
                }
            }
            let count = (SS * SS) as f32;
            let alpha = acc[3] / count;
            let i = (py as usize * n + px as usize) * 4;
            if alpha > 0.0 {
                for ch in 0..3 {
                    out[i + ch] = (acc[ch] / acc[3] * 255.0).round().clamp(0.0, 255.0) as u8;
                }
                out[i + 3] = (alpha * 255.0).round() as u8;
            }
        }
    }
    out
}

/// Encode the icon as a multi-resolution `.ico` (32-bit BMP entries).
/// qview stores the 256 px layer as PNG through the `png` crate instead:
/// ~50 KB an icon instead of ~306 KB.
pub fn ico(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<Vec<u8>> = sizes.iter().map(|&s| bmp_entry(s)).collect();
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]); // reserved, type = icon
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&s, img) in sizes.iter().zip(&images) {
        let dim = if s >= 256 { 0 } else { s as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bpp
        out.extend_from_slice(&(img.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += img.len() as u32;
    }
    for img in images {
        out.extend_from_slice(&img);
    }
    out
}

/// BITMAPINFOHEADER + bottom-up BGRA pixels + 1-bpp AND mask.
fn bmp_entry(size: u32) -> Vec<u8> {
    let px = rgba(size);
    let n = size as usize;
    let mask_stride = n.div_ceil(32) * 4;
    let mut out = Vec::with_capacity(40 + n * n * 4 + mask_stride * n);
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(size as i32).to_le_bytes());
    out.extend_from_slice(&(2 * size as i32).to_le_bytes()); // XOR + AND
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]); // compression .. colours important
    for y in (0..n).rev() {
        for x in 0..n {
            let i = (y * n + x) * 4;
            out.extend_from_slice(&[px[i + 2], px[i + 1], px[i], px[i + 3]]);
        }
    }
    // Alpha channel carries transparency; an all-zero AND mask is correct.
    out.resize(out.len() + mask_stride * n, 0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_shape() {
        let px = rgba(64);
        assert_eq!(px.len(), 64 * 64 * 4);
        // The corner is transparent, the centre opaque.
        assert_eq!(px[3], 0);
        let c = ((32 * 64) + 32) * 4;
        assert_eq!(px[c + 3], 255);
        let ico = ico(&[16, 32]);
        assert_eq!(&ico[..6], &[0, 0, 1, 0, 2, 0]);
    }
}
