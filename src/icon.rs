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

/// The colour of the icon at `(x, y)`, both in 0..1 of the icon's side, for
/// an icon of `size` pixels: a brushed-metal rounded square with a blue
/// disc and a white Play triangle.
fn sample(x: f32, y: f32, size: u32) -> Rgba {
    // Anti-aliasing width in icon units: a pixel at any size (a fixed width
    // in icon units blurs the edges of the large sizes).
    let aa = 0.5 / size as f32;
    let mut c = [0.0; 4];

    // The plate: a vertical steel gradient with faint horizontal brushing.
    let plate = rounded_box(x, y, (0.04, 0.04), (0.96, 0.96), 0.2);
    let brushing = ((y * 97.0).sin() * 0.5 + 0.5) * 0.025;
    let steel = mix(hex(0x5b626d), hex(0x1b1e23), y);
    let steel = [
        steel[0] + brushing,
        steel[1] + brushing,
        steel[2] + brushing,
        1.0,
    ];
    c = over(c, with_alpha(steel, smoothstep(aa, -aa, plate)));
    // A light rim along the top edge, a dark one along the bottom.
    let rim = smoothstep(0.035, 0.0, -plate) * smoothstep(aa, -aa, plate);
    c = over(
        c,
        with_alpha(mix(hex(0xc9ced6), hex(0x0c0d10), y), rim * 0.7),
    );

    // The disc: blue, lighter at the top.
    let (cx, cy, r) = (0.5, 0.5, 0.32);
    let disc = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() - r;
    c = over(
        c,
        with_alpha(hex(0x000000), smoothstep(0.06, -0.02, disc - 0.02) * 0.35),
    );
    let blue = mix(hex(0x5aa2ff), hex(0x1a56c4), (y - (cy - r)) / (2.0 * r));
    c = over(c, with_alpha(blue, smoothstep(aa, -aa, disc)));
    // A highlight across the upper half of the disc.
    let shine = ((x - cx).powi(2) + (y - (cy - 0.13)).powi(2) * 2.2).sqrt() - 0.22;
    c = over(
        c,
        with_alpha(
            hex(0xffffff),
            smoothstep(aa, -aa, shine) * smoothstep(aa, -aa, disc) * 0.16,
        ),
    );

    // Play, its centre of mass a little right of the disc's centre.
    let play = triangle(x, y, (0.43, 0.355), (0.645, 0.5), (0.43, 0.645)) - 0.012;
    c = over(c, with_alpha(hex(0xffffff), smoothstep(aa, -aa, play)));
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
