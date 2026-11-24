use image::imageops::FilterType;
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StencilParams {
    pub bg_mode: String,           // "auto_border", "none"
    pub bg_tolerance: i32,         // 5..100
    pub invert: bool,
    pub contrast: f32,             // 0.5..2.5
    pub smooth_strength: i32,      // 0..4

    pub style_mode: String,        // "art_hybrid", "adaptive", "threshold", "edges_only"
    pub shadow_threshold: i32,     // 10..245
    pub edge_sensitivity: i32,     // 0..100
    pub line_thickness_mm: f32,    // 0.8..5.0
    pub strengthen_outer: bool,
    pub outer_contour_mm: f32,     // 1.0..5.0

    pub island_mode: String,       // "bridge", "remove"
    pub bridge_width_mm: f32,      // 1.0..5.0
    pub max_bridge_length_mm: f32, // 2.0..80.0
    pub min_island_area_mm2: f32,  // 0.5..60.0
    pub min_hole_area_mm2: f32,    // 0.0..30.0
    pub wall_offset_mm: f32,       // -1.0..2.0
    pub prune_spurs_mm: f32,       // 0.0..1.5
    pub simplify_tol_mm: f32,      // 0.05..0.8

    pub target_height_mm: f32,     // 30..400
    pub thickness_mm: f32,         // 1.2..20.0
    pub stand_mode: String,        // "slotted", "assembled", "none"
    pub add_bottom_bar: bool,
    pub stand_depth_mm: f32,       // 15..80
    pub stand_height_mm: f32,      // 5..25
    pub slot_depth_mm: f32,        // 2.5..12.0
    pub slot_clearance_mm: f32,    // 0.05..0.8
    pub stand_margin_mm: f32,      // 5..40
}

impl Default for StencilParams {
    fn default() -> Self {
        Self {
            bg_mode: "auto_border".into(),
            bg_tolerance: 32,
            invert: false,
            contrast: 1.15,
            smooth_strength: 2,
            style_mode: "art_hybrid".into(),
            shadow_threshold: 92,
            edge_sensitivity: 55,
            line_thickness_mm: 1.8,
            strengthen_outer: true,
            outer_contour_mm: 2.0,
            island_mode: "bridge".into(),
            bridge_width_mm: 2.0,
            max_bridge_length_mm: 18.0,
            min_island_area_mm2: 6.0,
            min_hole_area_mm2: 3.5,
            wall_offset_mm: 0.0,
            prune_spurs_mm: 0.30,
            simplify_tol_mm: 0.18,
            target_height_mm: 180.0,
            thickness_mm: 4.0,
            stand_mode: "slotted".into(),
            add_bottom_bar: true,
            stand_depth_mm: 32.0,
            stand_height_mm: 10.0,
            slot_depth_mm: 6.0,
            slot_clearance_mm: 0.25,
            stand_margin_mm: 14.0,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessStats {
    pub connected_components: usize,
    pub open_contours: usize,
    pub islands_in_holes_remaining: usize,
    pub bridges_added: usize,
    pub islands_removed: usize,
    pub holes_count: usize,
    pub width_mm: f64,
    pub height_mm: f64,
    pub thickness_mm: f64,
    pub vertex_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct StencilModel {
    pub outer: Vec<[f64; 2]>,             // CCW
    pub holes: Vec<Vec<[f64; 2]>>,        // CW
    pub bridges: Vec<Vec<[f64; 2]>>,      // Визуализация мостов
    pub removed: Vec<Vec<[f64; 2]>>,      // Визуализация удалённых островков
    pub bottom_bar: Option<[f64; 4]>,     // [x0, y0, x1, y1]
    pub stand_span_x: [f64; 2],
    pub stand_box_x: [f64; 2],
    pub img_bounds_mm: [f64; 4],          // [x0, y0, x1, y1]
    pub bounds_mm: [f64; 4],              // [minx, miny, maxx, maxy]
    pub stats: ProcessStats,
}

pub fn decode_and_downscale(bytes: &[u8], max_side: u32) -> Result<RgbaImage, String> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| format!("Не удалось прочитать изображение: {e}"))?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    if w < 8 || h < 8 {
        return Err("Изображение слишком маленькое.".into());
    }
    if w.max(h) <= max_side {
        return Ok(rgba);
    }
    let scale = max_side as f32 / w.max(h) as f32;
    let nw = ((w as f32 * scale).round() as u32).max(16);
    let nh = ((h as f32 * scale).round() as u32).max(16);
    Ok(image::imageops::resize(&rgba, nw, nh, FilterType::Triangle))
}

// ---------------------------------------------------------------------------
// Точное 2D преобразование расстояний (Felzenszwalb & Huttenlocher, O(W*H))
// ---------------------------------------------------------------------------
fn edt_1d(f: &[f32], out: &mut [f32], v: &mut [usize], z: &mut [f32]) {
    let n = f.len();
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f32::NEG_INFINITY;
    z[1] = f32::INFINITY;
    for q in 1..n {
        let mut s = ((f[q] + (q * q) as f32) - (f[v[k]] + (v[k] * v[k]) as f32))
            / (2.0 * (q as f32 - v[k] as f32));
        while s <= z[k] {
            if k == 0 {
                break;
            }
            k -= 1;
            s = ((f[q] + (q * q) as f32) - (f[v[k]] + (v[k] * v[k]) as f32))
                / (2.0 * (q as f32 - v[k] as f32));
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f32::INFINITY;
    }
    let mut k = 0usize;
    for q in 0..n {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let d = q as f32 - v[k] as f32;
        out[q] = d * d + f[v[k]];
    }
}

/// Возвращает квадрат евклидова расстояния до ближайшего пикселя, где `mask[i] == target`.
fn edt_sq(mask: &[u8], w: usize, h: usize, target: u8) -> Vec<f32> {
    const INF: f32 = 1e12;
    let mut g = vec![INF; w * h];
    let mut col_in = vec![0.0f32; h];
    let mut col_out = vec![0.0f32; h];
    let mut v = vec![0usize; w.max(h) + 1];
    let mut z = vec![0.0f32; w.max(h) + 2];

    for x in 0..w {
        for y in 0..h {
            col_in[y] = if mask[y * w + x] == target { 0.0 } else { INF };
        }
        edt_1d(&col_in, &mut col_out, &mut v, &mut z);
        for y in 0..h {
            g[y * w + x] = col_out[y];
        }
    }

    let mut out = vec![INF; w * h];
    let mut row_out = vec![0.0f32; w];
    for y in 0..h {
        let row = &g[y * w..(y + 1) * w];
        edt_1d(row, &mut row_out, &mut v, &mut z);
        out[y * w..(y + 1) * w].copy_from_slice(&row_out);
    }
    out
}

fn dilate_disk(mask: &[u8], w: usize, h: usize, radius: f32) -> Vec<u8> {
    if radius <= 0.4 {
        return mask.to_vec();
    }
    let r2 = radius * radius;
    edt_sq(mask, w, h, 1)
        .into_iter()
        .map(|d2| u8::from(d2 <= r2))
        .collect()
}

fn erode_disk(mask: &[u8], w: usize, h: usize, radius: f32) -> Vec<u8> {
    if radius <= 0.4 {
        return mask.to_vec();
    }
    let r2 = radius * radius;
    edt_sq(mask, w, h, 0)
        .into_iter()
        .map(|d2| u8::from(d2 > r2))
        .collect()
}

// ---------------------------------------------------------------------------
// CIE Lab + удаление фона от краёв изображения
// ---------------------------------------------------------------------------
fn rgb_to_lab(r: u8, g: u8, b: u8) -> [f32; 3] {
    fn lin(c: u8) -> f32 {
        let v = c as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    fn f(t: f32) -> f32 {
        if t > 0.008856 {
            t.cbrt()
        } else {
            7.787 * t + 16.0 / 116.0
        }
    }
    let rl = lin(r);
    let gl = lin(g);
    let bl = lin(b);
    let x = (0.4124564 * rl + 0.3575761 * gl + 0.1804375 * bl) / 0.95047;
    let y = 0.2126729 * rl + 0.7151522 * gl + 0.0721750 * bl;
    let z = (0.0193339 * rl + 0.1191920 * gl + 0.9503041 * bl) / 1.08883;
    let fx = f(x);
    let fy = f(y);
    let fz = f(z);
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

fn median_lab(samples: &[[f32; 3]]) -> [f32; 3] {
    if samples.is_empty() {
        return [50.0, 0.0, 0.0];
    }
    let mut out = [0.0f32; 3];
    for c in 0..3 {
        let mut vals: Vec<f32> = samples.iter().map(|s| s[c]).collect();
        vals.sort_by(|a, b| a.total_cmp(b));
        out[c] = vals[vals.len() / 2];
    }
    out
}

fn compute_foreground_mask(rgba: &RgbaImage, bg_mode: &str, bg_tolerance: i32) -> Option<Vec<u8>> {
    let w = rgba.width() as usize;
    let h = rgba.height() as usize;
    let raw = rgba.as_raw();

    if bg_mode == "none" {
        return None;
    }

    // Если у PNG есть прозрачность — используем альфа-канал
    let mut transp = 0usize;
    for i in 0..w * h {
        if raw[i * 4 + 3] < 128 {
            transp += 1;
        }
    }
    let tr = transp as f32 / (w * h) as f32;
    if tr > 0.01 && tr < 0.98 {
        let mut fg = vec![0u8; w * h];
        for i in 0..w * h {
            fg[i] = u8::from(raw[i * 4 + 3] >= 128);
        }
        return Some(fg);
    }

    // Заливка фона от краёв в пространстве CIE Lab
    let mut lab = vec![[0.0f32; 3]; w * h];
    for i in 0..w * h {
        lab[i] = rgb_to_lab(raw[i * 4], raw[i * 4 + 1], raw[i * 4 + 2]);
    }

    let mut border = Vec::with_capacity(2 * (w + h));
    for x in 0..w {
        border.push(lab[x]);
        border.push(lab[(h - 1) * w + x]);
    }
    for y in 0..h {
        border.push(lab[y * w]);
        border.push(lab[y * w + (w - 1)]);
    }

    let mut refs = vec![median_lab(&border)];
    let patch = (w.min(h) / 25).max(4);
    for &(cy0, cx0) in &[
        (0, 0),
        (0, w - patch),
        (h - patch, 0),
        (h - patch, w - patch),
    ] {
        let mut pts = Vec::with_capacity(patch * patch);
        for y in cy0..cy0 + patch {
            for x in cx0..cx0 + patch {
                pts.push(lab[y * w + x]);
            }
        }
        refs.push(median_lab(&pts));
    }

    let thresh = 6.0 + bg_tolerance as f32 * 0.72;
    let thresh2 = thresh * thresh;
    let mut bg_cand = vec![0u8; w * h];
    for i in 0..w * h {
        let p = lab[i];
        let mut best = f32::INFINITY;
        for r in &refs {
            let d2 = (p[0] - r[0]).powi(2) + (p[1] - r[1]).powi(2) + (p[2] - r[2]).powi(2);
            if d2 < best {
                best = d2;
            }
        }
        if best <= thresh2 {
            bg_cand[i] = 1;
        }
    }

    // BFS только от граничных пикселей, чтобы не затронуть светлые области ВНУТРИ объекта
    let mut is_bg = vec![0u8; w * h];
    let mut q = VecDeque::new();
    for x in 0..w {
        for &y in &[0usize, h - 1] {
            let idx = y * w + x;
            if bg_cand[idx] == 1 && is_bg[idx] == 0 {
                is_bg[idx] = 1;
                q.push_back((x, y));
            }
        }
    }
    for y in 0..h {
        for &x in &[0usize, w - 1] {
            let idx = y * w + x;
            if bg_cand[idx] == 1 && is_bg[idx] == 0 {
                is_bg[idx] = 1;
                q.push_back((x, y));
            }
        }
    }
    while let Some((x, y)) = q.pop_front() {
        for (nx, ny) in [
            (x.wrapping_sub(1), y),
            (x + 1, y),
            (x, y.wrapping_sub(1)),
            (x, y + 1),
        ] {
            if nx < w && ny < h {
                let ni = ny * w + nx;
                if bg_cand[ni] == 1 && is_bg[ni] == 0 {
                    is_bg[ni] = 1;
                    q.push_back((nx, ny));
                }
            }
        }
    }

    let mut fg: Vec<u8> = is_bg.into_iter().map(|b| 1 - b).collect();
    // Лёгкое сглаживание краёв маски
    fg = erode_disk(&dilate_disk(&fg, w, h, 2.0), w, h, 2.0);
    fg = dilate_disk(&erode_disk(&fg, w, h, 1.5), w, h, 1.5);

    let fg_cnt: usize = fg.iter().map(|&v| v as usize).sum();
    let ratio = fg_cnt as f32 / (w * h) as f32;
    if ratio < 0.015 || ratio > 0.99 {
        None
    } else {
        Some(fg)
    }
}

// ---------------------------------------------------------------------------
// Фильтры: билатеральное сглаживание, адаптивный порог, детектор границ Canny
// ---------------------------------------------------------------------------
fn bilateral_filter(gray: &[f32], w: usize, h: usize, strength: i32) -> Vec<f32> {
    if strength <= 0 {
        return gray.to_vec();
    }
    let rad = (strength as isize).clamp(1, 3);
    let sigma_s = 1.5 + strength as f32 * 0.8;
    let sigma_r = 20.0 + strength as f32 * 14.0;
    let inv_2s2 = 1.0 / (2.0 * sigma_s * sigma_s);
    let inv_2r2 = 1.0 / (2.0 * sigma_r * sigma_r);

    let mut out = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let center = gray[y * w + x];
            let mut sum = 0.0f32;
            let mut wsum = 0.0f32;
            for dy in -rad..=rad {
                let ny = (y as isize + dy).clamp(0, h as isize - 1) as usize;
                for dx in -rad..=rad {
                    let nx = (x as isize + dx).clamp(0, w as isize - 1) as usize;
                    let val = gray[ny * w + nx];
                    let ds = (dx * dx + dy * dy) as f32;
                    let dr = val - center;
                    let weight = (-(ds * inv_2s2 + dr * dr * inv_2r2)).exp();
                    sum += val * weight;
                    wsum += weight;
                }
            }
            out[y * w + x] = sum / wsum.max(1e-6);
        }
    }
    out
}

fn adaptive_threshold_inv(gray: &[f32], w: usize, h: usize, rad: usize, c_val: f32) -> Vec<u8> {
    let mut sat = vec![0.0f64; (w + 1) * (h + 1)];
    for y in 0..h {
        let mut row_sum = 0.0f64;
        for x in 0..w {
            row_sum += gray[y * w + x] as f64;
            sat[(y + 1) * (w + 1) + (x + 1)] = sat[y * (w + 1) + (x + 1)] + row_sum;
        }
    }
    let mut out = vec![0u8; w * h];
    for y in 0..h {
        let y0 = y.saturating_sub(rad);
        let y1 = (y + rad + 1).min(h);
        for x in 0..w {
            let x0 = x.saturating_sub(rad);
            let x1 = (x + rad + 1).min(w);
            let area = ((x1 - x0) * (y1 - y0)) as f64;
            let sum = sat[y1 * (w + 1) + x1]
                - sat[y0 * (w + 1) + x1]
                - sat[y1 * (w + 1) + x0]
                + sat[y0 * (w + 1) + x0];
            let mean = (sum / area) as f32;
            if gray[y * w + x] < mean - c_val {
                out[y * w + x] = 1;
            }
        }
    }
    out
}

fn canny_edges(gray: &[f32], w: usize, h: usize, low_t: f32, high_t: f32) -> Vec<u8> {
    let mut mag = vec![0.0f32; w * h];
    let mut dir = vec![0u8; w * h];
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let p00 = gray[(y - 1) * w + (x - 1)];
            let p01 = gray[(y - 1) * w + x];
            let p02 = gray[(y - 1) * w + (x + 1)];
            let p10 = gray[y * w + (x - 1)];
            let p12 = gray[y * w + (x + 1)];
            let p20 = gray[(y + 1) * w + (x - 1)];
            let p21 = gray[(y + 1) * w + x];
            let p22 = gray[(y + 1) * w + (x + 1)];

            let gx = -p00 + p02 - 2.0 * p10 + 2.0 * p12 - p20 + p22;
            let gy = -p00 - 2.0 * p01 - p02 + p20 + 2.0 * p21 + p22;
            let m = (gx * gx + gy * gy).sqrt() * 0.25;
            mag[y * w + x] = m;

            let mut ang = gy.atan2(gx).to_degrees();
            if ang < 0.0 {
                ang += 180.0;
            }
            dir[y * w + x] = if !(22.5..157.5).contains(&ang) {
                0
            } else if ang < 67.5 {
                1
            } else if ang < 112.5 {
                2
            } else {
                3
            };
        }
    }

    // Non-maximum suppression + гистерезис
    let mut state = vec![0u8; w * h]; // 0=нет, 1=слабый, 2=сильный
    let mut q = VecDeque::new();
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = y * w + x;
            let m = mag[i];
            if m < low_t {
                continue;
            }
            let (m1, m2) = match dir[i] {
                0 => (mag[y * w + x - 1], mag[y * w + x + 1]),
                1 => (mag[(y - 1) * w + x - 1], mag[(y + 1) * w + x + 1]),
                2 => (mag[(y - 1) * w + x], mag[(y + 1) * w + x]),
                _ => (mag[(y - 1) * w + x + 1], mag[(y + 1) * w + x - 1]),
            };
            if m >= m1 && m >= m2 {
                if m >= high_t {
                    state[i] = 2;
                    q.push_back((x, y));
                } else {
                    state[i] = 1;
                }
            }
        }
    }

    while let Some((x, y)) = q.pop_front() {
        for dy in -1isize..=1 {
            for dx in -1isize..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = (x as isize + dx) as usize;
                let ny = (y as isize + dy) as usize;
                if nx < w && ny < h {
                    let ni = ny * w + nx;
                    if state[ni] == 1 {
                        state[ni] = 2;
                        q.push_back((nx, ny));
                    }
                }
            }
        }
    }

    state.into_iter().map(|s| u8::from(s == 2)).collect()
}

// ---------------------------------------------------------------------------
// Связные компоненты и авто-мосты (MST)
// ---------------------------------------------------------------------------
fn label_components(mask: &[u8], w: usize, h: usize, target: u8, conn8: bool) -> (Vec<u32>, Vec<usize>) {
    let mut labels = vec![0u32; w * h];
    let mut sizes = Vec::new();
    let mut q = VecDeque::new();
    let mut cur = 0u32;

    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            if mask[idx] == target && labels[idx] == 0 {
                cur += 1;
                labels[idx] = cur;
                q.push_back((x, y));
                let mut cnt = 0usize;
                while let Some((cx, cy)) = q.pop_front() {
                    cnt += 1;
                    let neigh4 = [
                        (cx.wrapping_sub(1), cy),
                        (cx + 1, cy),
                        (cx, cy.wrapping_sub(1)),
                        (cx, cy + 1),
                    ];
                    for (nx, ny) in neigh4 {
                        if nx < w && ny < h {
                            let ni = ny * w + nx;
                            if mask[ni] == target && labels[ni] == 0 {
                                labels[ni] = cur;
                                q.push_back((nx, ny));
                            }
                        }
                    }
                    if conn8 {
                        let neigh_diag = [
                            (cx.wrapping_sub(1), cy.wrapping_sub(1)),
                            (cx + 1, cy.wrapping_sub(1)),
                            (cx.wrapping_sub(1), cy + 1),
                            (cx + 1, cy + 1),
                        ];
                        for (nx, ny) in neigh_diag {
                            if nx < w && ny < h {
                                let ni = ny * w + nx;
                                if mask[ni] == target && labels[ni] == 0 {
                                    labels[ni] = cur;
                                    q.push_back((nx, ny));
                                }
                            }
                        }
                    }
                }
                sizes.push(cnt);
            }
        }
    }
    (labels, sizes)
}

fn draw_capsule(mask: &mut [u8], w: usize, h: usize, p1: (f32, f32), p2: (f32, f32), radius: f32) {
    let x0 = ((p1.0.min(p2.0) - radius - 1.0).floor() as isize).clamp(0, w as isize - 1) as usize;
    let x1 = ((p1.0.max(p2.0) + radius + 1.0).ceil() as isize).clamp(0, w as isize - 1) as usize;
    let y0 = ((p1.1.min(p2.1) - radius - 1.0).floor() as isize).clamp(0, h as isize - 1) as usize;
    let y1 = ((p1.1.max(p2.1) + radius + 1.0).ceil() as isize).clamp(0, h as isize - 1) as usize;

    let vx = p2.0 - p1.0;
    let vy = p2.1 - p1.1;
    let len2 = (vx * vx + vy * vy).max(1e-6);
    let r2 = radius * radius;

    for y in y0..=y1 {
        for x in x0..=x1 {
            let px = x as f32;
            let py = y as f32;
            let t = (((px - p1.0) * vx + (py - p1.1) * vy) / len2).clamp(0.0, 1.0);
            let cx = p1.0 + t * vx;
            let cy = p1.1 + t * vy;
            let d2 = (px - cx).powi(2) + (py - cy).powi(2);
            if d2 <= r2 {
                mask[y * w + x] = 1;
            }
        }
    }
}

fn capsule_polygon(p1: [f64; 2], p2: [f64; 2], r: f64) -> Vec<[f64; 2]> {
    let dx = p2[0] - p1[0];
    let dy = p2[1] - p1[1];
    let base_ang = dy.atan2(dx);
    let steps = 6;
    let mut ring = Vec::with_capacity(steps * 2 + 2);
    for i in 0..=steps {
        let a = base_ang - std::f64::consts::FRAC_PI_2
            + std::f64::consts::PI * (i as f64 / steps as f64);
        ring.push([p2[0] + r * a.cos(), p2[1] + r * a.sin()]);
    }
    for i in 0..=steps {
        let a = base_ang + std::f64::consts::FRAC_PI_2
            + std::f64::consts::PI * (i as f64 / steps as f64);
        ring.push([p1[0] + r * a.cos(), p1[1] + r * a.sin()]);
    }
    ring
}

fn subsample_pts(pts: &[(usize, usize)], max_n: usize) -> Vec<(usize, usize)> {
    if pts.len() <= max_n {
        return pts.to_vec();
    }
    let step = (pts.len() as f32 / max_n as f32).ceil() as usize;
    pts.iter().step_by(step.max(1)).copied().collect()
}

// ---------------------------------------------------------------------------
// Субпиксельный Marching Squares -> строго замкнутые простые контуры
// ---------------------------------------------------------------------------
fn marching_squares_rings(mask: &[u8], w: usize, h: usize) -> Vec<Vec<[f64; 2]>> {
    // 1) Добавляем рамку из нулей (3 px) и делаем мягкий 3x3 Gaussian blur
    let pad = 3usize;
    let pw = w + 2 * pad;
    let ph = h + 2 * pad;
    let mut tmp = vec![0.0f32; pw * ph];
    let mut field = vec![0.0f32; pw * ph];

    for y in 0..h {
        for x in 0..w {
            field[(y + pad) * pw + (x + pad)] = if mask[y * w + x] != 0 { 255.0 } else { 0.0 };
        }
    }
    // Горизонтальный [1,2,1]/4
    for y in 1..ph - 1 {
        for x in 1..pw - 1 {
            tmp[y * pw + x] = 0.25 * field[y * pw + x - 1]
                + 0.50 * field[y * pw + x]
                + 0.25 * field[y * pw + x + 1];
        }
    }
    // Вертикальный [1,2,1]/4
    field.fill(0.0);
    for y in 1..ph - 1 {
        for x in 1..pw - 1 {
            field[y * pw + x] = 0.25 * tmp[(y - 1) * pw + x]
                + 0.50 * tmp[y * pw + x]
                + 0.25 * tmp[(y + 1) * pw + x];
        }
    }

    const T: f32 = 127.5;
    type EdgeKey = (u8, usize, usize); // (0=horiz,1=vert, x, y)
    let mut next_edge: HashMap<EdgeKey, EdgeKey> = HashMap::new();
    let mut edge_pos: HashMap<EdgeKey, [f64; 2]> = HashMap::new();

    let interp = |va: f32, vb: f32| -> f64 {
        let d = vb - va;
        if d.abs() < 1e-6 {
            0.5
        } else {
            ((T - va) / d).clamp(0.03, 0.97) as f64
        }
    };

    for y in 0..ph - 1 {
        for x in 0..pw - 1 {
            let v_tl = field[y * pw + x];
            let v_tr = field[y * pw + x + 1];
            let v_br = field[(y + 1) * pw + x + 1];
            let v_bl = field[(y + 1) * pw + x];

            let idx = (u8::from(v_tl >= T) << 3)
                | (u8::from(v_tr >= T) << 2)
                | (u8::from(v_br >= T) << 1)
                | u8::from(v_bl >= T);

            if idx == 0 || idx == 15 {
                continue;
            }

            let avg = 0.25 * (v_tl + v_tr + v_br + v_bl);
            let segs: &[(u8, u8)] = match idx {
                1 => &[(2, 3)],
                2 => &[(1, 2)],
                3 => &[(1, 3)],
                4 => &[(0, 1)],
                5 => if avg >= T { &[(0, 3), (2, 1)] } else { &[(0, 1), (2, 3)] },
                6 => &[(0, 2)],
                7 => &[(0, 3)],
                8 => &[(3, 0)],
                9 => &[(2, 0)],
                10 => if avg >= T { &[(3, 2), (1, 0)] } else { &[(3, 0), (1, 2)] },
                11 => &[(1, 0)],
                12 => &[(3, 1)],
                13 => &[(2, 1)],
                14 => &[(3, 2)],
                _ => &[],
            };

            let edge_info = |e: u8| -> (EdgeKey, [f64; 2]) {
                match e {
                    0 => {
                        let t = interp(v_tl, v_tr);
                        ((0, x, y), [x as f64 - pad as f64 + t, (h as f64) - (y as f64 - pad as f64)])
                    }
                    1 => {
                        let t = interp(v_tr, v_br);
                        ((1, x + 1, y), [(x + 1) as f64 - pad as f64, (h as f64) - (y as f64 - pad as f64 + t)])
                    }
                    2 => {
                        let t = interp(v_bl, v_br);
                        ((0, x, y + 1), [x as f64 - pad as f64 + t, (h as f64) - ((y + 1) as f64 - pad as f64)])
                    }
                    _ => {
                        let t = interp(v_tl, v_bl);
                        ((1, x, y), [x as f64 - pad as f64, (h as f64) - (y as f64 - pad as f64 + t)])
                    }
                }
            };

            for &(a, b) in segs {
                let (ka, pa) = edge_info(a);
                let (kb, pb) = edge_info(b);
                edge_pos.entry(ka).or_insert(pa);
                edge_pos.entry(kb).or_insert(pb);
                next_edge.insert(ka, kb);
            }
        }
    }

    // Собираем замкнутые циклы
    let mut visited = HashMap::new();
    let mut rings = Vec::new();
    let keys: Vec<EdgeKey> = next_edge.keys().copied().collect();
    for start in keys {
        if visited.contains_key(&start) {
            continue;
        }
        let mut ring = Vec::new();
        let mut cur = start;
        let mut closed = false;
        while !visited.contains_key(&cur) {
            visited.insert(cur, true);
            if let Some(&pos) = edge_pos.get(&cur) {
                ring.push(pos);
            }
            if let Some(&nxt) = next_edge.get(&cur) {
                cur = nxt;
                if cur == start {
                    closed = true;
                    break;
                }
            } else {
                break;
            }
        }
        if closed && ring.len() >= 3 {
            rings.push(ring);
        }
    }
    rings
}

// ---------------------------------------------------------------------------
// Геометрия полигонов: площадь, точка-в-полигоне, безопасное упрощение RDP
// ---------------------------------------------------------------------------
pub fn signed_area(ring: &[[f64; 2]]) -> f64 {
    let n = ring.len();
    if n < 3 {
        return 0.0;
    }
    let mut s = 0.0;
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        s += a[0] * b[1] - b[0] * a[1];
    }
    0.5 * s
}

fn point_in_ring(pt: [f64; 2], ring: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let n = ring.len();
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        if ((a[1] > pt[1]) != (b[1] > pt[1]))
            && (pt[0] < (b[0] - a[0]) * (pt[1] - a[1]) / (b[1] - a[1]) + a[0])
        {
            inside = !inside;
        }
    }
    inside
}

fn rdp_open(pts: &[[f64; 2]], eps: f64, out: &mut Vec<[f64; 2]>) {
    if pts.len() <= 2 {
        out.push(pts[0]);
        return;
    }
    let a = pts[0];
    let b = pts[pts.len() - 1];
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len = (dx * dx + dy * dy).sqrt();
    let mut best_d = -1.0f64;
    let mut best_i = 0usize;
    for (i, &p) in pts.iter().enumerate().skip(1).take(pts.len() - 2) {
        let d = if len < 1e-9 {
            ((p[0] - a[0]).powi(2) + (p[1] - a[1]).powi(2)).sqrt()
        } else {
            ((dy * p[0] - dx * p[1] + b[0] * a[1] - b[1] * a[0]).abs()) / len
        };
        if d > best_d {
            best_d = d;
            best_i = i;
        }
    }
    if best_d > eps {
        rdp_open(&pts[..=best_i], eps, out);
        rdp_open(&pts[best_i..], eps, out);
    } else {
        out.push(a);
    }
}

fn simplify_closed_ring(ring: &[[f64; 2]], eps: f64) -> Vec<[f64; 2]> {
    if ring.len() <= 4 || eps <= 1e-4 {
        return ring.to_vec();
    }
    // Находим самую удалённую пару вершин (инвариантно к начальной точке)
    let p0 = ring[0];
    let mut i_far = 0usize;
    let mut max_d2 = -1.0f64;
    for (i, &p) in ring.iter().enumerate() {
        let d2 = (p[0] - p0[0]).powi(2) + (p[1] - p0[1]).powi(2);
        if d2 > max_d2 {
            max_d2 = d2;
            i_far = i;
        }
    }
    if i_far == 0 || i_far >= ring.len() - 1 {
        return ring.to_vec();
    }

    let mut out = Vec::new();
    rdp_open(&ring[..=i_far], eps, &mut out);
    let mut second = ring[i_far..].to_vec();
    second.push(ring[0]);
    rdp_open(&second, eps, &mut out);

    // Убираем микро-сегменты < 0.03 мм
    let mut cleaned: Vec<[f64; 2]> = Vec::with_capacity(out.len());
    for p in out {
        if let Some(&last) = cleaned.last() {
            if (p[0] - last[0]).hypot(p[1] - last[1]) < 0.03 {
                continue;
            }
        }
        cleaned.push(p);
    }
    while cleaned.len() > 3 {
        let a = cleaned[0];
        let b = *cleaned.last().unwrap();
        if (a[0] - b[0]).hypot(a[1] - b[1]) < 0.03 {
            cleaned.pop();
        } else {
            break;
        }
    }
    if cleaned.len() >= 3 {
        cleaned
    } else {
        ring.to_vec()
    }
}

fn seg_intersect(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let bb_ok = a[0].max(b[0]) >= c[0].min(d[0]) - 1e-9
        && c[0].max(d[0]) >= a[0].min(b[0]) - 1e-9
        && a[1].max(b[1]) >= c[1].min(d[1]) - 1e-9
        && c[1].max(d[1]) >= a[1].min(b[1]) - 1e-9;
    if !bb_ok {
        return false;
    }
    let cross = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    };
    let d1 = cross(a, b, c);
    let d2 = cross(a, b, d);
    let d3 = cross(c, d, a);
    let d4 = cross(c, d, b);
    ((d1 > 1e-9 && d2 < -1e-9) || (d1 < -1e-9 && d2 > 1e-9))
        && ((d3 > 1e-9 && d4 < -1e-9) || (d3 < -1e-9 && d4 > 1e-9))
}

fn ring_self_intersects(r: &[[f64; 2]]) -> bool {
    let n = r.len();
    for i in 0..n {
        let a = r[i];
        let b = r[(i + 1) % n];
        for j in (i + 2)..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            if seg_intersect(a, b, r[j], r[(j + 1) % n]) {
                return true;
            }
        }
    }
    false
}

fn rings_intersect(r1: &[[f64; 2]], r2: &[[f64; 2]]) -> bool {
    let n1 = r1.len();
    let n2 = r2.len();
    for i in 0..n1 {
        let a = r1[i];
        let b = r1[(i + 1) % n1];
        for j in 0..n2 {
            if seg_intersect(a, b, r2[j], r2[(j + 1) % n2]) {
                return true;
            }
        }
    }
    false
}

fn safe_simplify(outer: &[[f64; 2]], holes: &[Vec<[f64; 2]>], tol: f64) -> (Vec<[f64; 2]>, Vec<Vec<[f64; 2]>>) {
    let mut eps = tol;
    for _ in 0..4 {
        let so = simplify_closed_ring(outer, eps);
        let sh: Vec<Vec<[f64; 2]>> = holes.iter().map(|h| simplify_closed_ring(h, eps)).collect();
        let mut ok = signed_area(&so) > 1e-2 && !ring_self_intersects(&so);
        if ok {
            for (i, h) in sh.iter().enumerate() {
                if signed_area(h) >= -1e-2 || ring_self_intersects(h) || rings_intersect(&so, h) {
                    ok = false;
                    break;
                }
                for h2 in sh.iter().skip(i + 1) {
                    if rings_intersect(h, h2) {
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    break;
                }
            }
        }
        if ok {
            return (so, sh);
        }
        eps *= 0.45;
    }
    (outer.to_vec(), holes.to_vec())
}

// ---------------------------------------------------------------------------
// Главный пайплайн обработки изображения
// ---------------------------------------------------------------------------
pub fn process_rgba(rgba: &RgbaImage, params: &StencilParams) -> Result<StencilModel, String> {
    let w = rgba.width() as usize;
    let h = rgba.height() as usize;
    let raw = rgba.as_raw();

    // 1. Маска объекта (удаление фона)
    let fg_opt = compute_foreground_mask(rgba, &params.bg_mode, params.bg_tolerance);

    // Оцениваем масштаб px/mm по высоте объекта
    let est_h_px = if let Some(ref fg) = fg_opt {
        let mut ymin = h;
        let mut ymax = 0usize;
        for y in 0..h {
            for x in 0..w {
                if fg[y * w + x] != 0 {
                    ymin = ymin.min(y);
                    ymax = ymax.max(y);
                }
            }
        }
        if ymax > ymin { (ymax - ymin + 1) as f32 } else { h as f32 }
    } else {
        h as f32
    };
    let px_per_mm = (est_h_px / params.target_height_mm.max(20.0)).max(0.5);

    // 2. Оттенки серого + контраст + инверсия + сглаживание
    let mut gray = vec![0.0f32; w * h];
    for i in 0..w * h {
        let r = raw[i * 4] as f32;
        let g = raw[i * 4 + 1] as f32;
        let b = raw[i * 4 + 2] as f32;
        let mut v = 0.299 * r + 0.587 * g + 0.114 * b;
        if let Some(ref fg) = fg_opt {
            if fg[i] == 0 {
                v = 255.0;
            }
        }
        if params.invert {
            v = 255.0 - v;
            if let Some(ref fg) = fg_opt {
                if fg[i] == 0 {
                    v = 255.0;
                }
            }
        }
        v = ((v - 128.0) * params.contrast + 128.0).clamp(0.0, 255.0);
        gray[i] = v;
    }
    let smooth = bilateral_filter(&gray, w, h, params.smooth_strength);

    // 3. Построение первичной маски трафарета по выбранному стилю
    let line_r = (params.line_thickness_mm * px_per_mm * 0.5).max(0.8);
    let mut solid = match params.style_mode.as_str() {
        "threshold" => {
            let t = params.shadow_threshold as f32;
            smooth.iter().map(|&v| u8::from(v <= t)).collect::<Vec<u8>>()
        }
        "adaptive" => {
            let rad = (9.0 * px_per_mm).clamp(5.0, 28.0) as usize;
            let c_val = (26.0 - params.edge_sensitivity as f32 * 0.22).max(2.0);
            let m = adaptive_threshold_inv(&smooth, w, h, rad, c_val);
            dilate_disk(&m, w, h, (line_r * 0.5).max(0.6))
        }
        "edges_only" => {
            let high_t = (95.0 - params.edge_sensitivity as f32 * 0.72).max(12.0);
            let low_t = high_t * 0.42;
            let edges = canny_edges(&smooth, w, h, low_t, high_t);
            dilate_disk(&edges, w, h, line_r)
        }
        _ => {
            // "art_hybrid": тёмные заливки + адаптивные штрихи + контуры Canny
            let t = params.shadow_threshold as f32;
            let dark: Vec<u8> = smooth.iter().map(|&v| u8::from(v <= t)).collect();

            let rad = (7.0 * px_per_mm).clamp(4.0, 22.0) as usize;
            let c_val = (27.0 - params.edge_sensitivity as f32 * 0.22).max(3.0);
            let ink = dilate_disk(
                &adaptive_threshold_inv(&smooth, w, h, rad, c_val),
                w,
                h,
                (line_r * 0.55).max(0.6),
            );

            let high_t = (100.0 - params.edge_sensitivity as f32 * 0.72).max(14.0);
            let low_t = high_t * 0.45;
            let edges = dilate_disk(&canny_edges(&smooth, w, h, low_t, high_t), w, h, line_r);

            (0..w * h)
                .map(|i| u8::from(dark[i] != 0 || ink[i] != 0 || edges[i] != 0))
                .collect()
        }
    };

    // Клип по маске объекта + усиление внешнего замкнутого контура силуэта
    if let Some(ref fg) = fg_opt {
        for i in 0..w * h {
            solid[i] &= fg[i];
        }
        if params.strengthen_outer {
            let border_r = (params.outer_contour_mm * px_per_mm).max(1.5);
            let dist_to_bg = edt_sq(fg, w, h, 0);
            let r2 = border_r * border_r;
            for i in 0..w * h {
                if fg[i] != 0 && dist_to_bg[i] <= r2 {
                    solid[i] = 1;
                }
            }
        }
    }

    // 4. Заделка микро-щелей + утолщение стенок + удаление тонких висящих шипов
    solid = erode_disk(&dilate_disk(&solid, w, h, 1.2), w, h, 1.2);
    if params.wall_offset_mm > 0.04 {
        solid = dilate_disk(&solid, w, h, params.wall_offset_mm * 0.5 * px_per_mm);
    } else if params.wall_offset_mm < -0.04 {
        let shrunk = erode_disk(&solid, w, h, (-params.wall_offset_mm) * 0.5 * px_per_mm);
        if shrunk.iter().any(|&v| v != 0) {
            solid = shrunk;
        }
    }
    if params.prune_spurs_mm > 0.04 {
        let r = params.prune_spurs_mm * px_per_mm;
        let opened = dilate_disk(&erode_disk(&solid, w, h, r), w, h, r);
        if opened.iter().any(|&v| v != 0) {
            solid = opened;
        }
    }

    // Расширяем холст вниз, если нужна нижняя планка под подставку
    let extra_bot = if params.stand_mode != "none" && params.add_bottom_bar {
        ((params.slot_depth_mm.min(6.0) + 4.0) * px_per_mm).ceil() as usize
    } else {
        0
    };
    let work_h = h + extra_bot;
    let mut work = vec![0u8; w * work_h];
    work[..w * h].copy_from_slice(&solid);

    // 5. Анализ связности -> удаление шума и авто-мосты (MST)
    let (labels, sizes) = label_components(&work, w, work_h, 1, false);
    if sizes.is_empty() {
        return Err("Не найдено ни одного контура. Измените порог теней или инвертируйте фото.".into());
    }

    let main_id = sizes
        .iter()
        .enumerate()
        .max_by_key(|&(_, sz)| *sz)
        .map(|(i, _)| (i + 1) as u32)
        .unwrap();

    // Находим границы каждой компоненты
    let mut comp_bounds: HashMap<u32, Vec<(usize, usize)>> = HashMap::new();
    for y in 0..work_h {
        for x in 0..w {
            let lbl = labels[y * w + x];
            if lbl == 0 {
                continue;
            }
            let is_edge = x == 0
                || y == 0
                || x + 1 == w
                || y + 1 == work_h
                || work[y * w + (x - 1)] == 0
                || work[y * w + (x + 1)] == 0
                || work[(y - 1) * w + x] == 0
                || work[(y + 1) * w + x] == 0;
            if is_edge {
                comp_bounds.entry(lbl).or_default().push((x, y));
            }
        }
    }

    let px2_per_mm2 = px_per_mm * px_per_mm;
    let min_isl_px = (params.min_island_area_mm2 * px2_per_mm2).max(4.0) as usize;

    let mut removed_ids: Vec<u32> = Vec::new();
    let mut candidates: Vec<(u32, Vec<(usize, usize)>)> = Vec::new();

    for (idx, &sz) in sizes.iter().enumerate() {
        let cid = (idx + 1) as u32;
        if cid == main_id {
            continue;
        }
        if params.island_mode == "remove" || sz < min_isl_px {
            removed_ids.push(cid);
        } else if let Some(bpts) = comp_bounds.remove(&cid) {
            candidates.push((cid, subsample_pts(&bpts, 350)));
        }
    }

    let mut connected_pts = subsample_pts(comp_bounds.get(&main_id).map(|v| v.as_slice()).unwrap_or(&[]), 1400);
    let max_bridge_px = params.max_bridge_length_mm * px_per_mm;
    let bridge_r_px = (params.bridge_width_mm * 0.5 * px_per_mm).max(1.2);
    let mut bridge_capsules_px: Vec<([f64; 2], [f64; 2], f64)> = Vec::new();

    while !candidates.is_empty() {
        let mut best_ci = None;
        let mut best_d2 = f32::INFINITY;
        let mut best_pair = ((0usize, 0usize), (0usize, 0usize));

        for (ci, (_, cpts)) in candidates.iter().enumerate() {
            for &p1 in &connected_pts {
                for &p2 in cpts {
                    let dx = p2.0 as f32 - p1.0 as f32;
                    let dy = p2.1 as f32 - p1.1 as f32;
                    let d2 = dx * dx + dy * dy;
                    if d2 < best_d2 {
                        best_d2 = d2;
                        best_ci = Some(ci);
                        best_pair = (p1, p2);
                    }
                }
            }
        }

        let dist = best_d2.sqrt();
        if best_ci.is_none() || dist > max_bridge_px {
            for (cid, _) in candidates.drain(..) {
                removed_ids.push(cid);
            }
            break;
        }

        let ci = best_ci.unwrap();
        let (_, cpts) = candidates.remove(ci);
        let (p1, p2) = best_pair;
        let (x1, y1) = (p1.0 as f32, p1.1 as f32);
        let (x2, y2) = (p2.0 as f32, p2.1 as f32);
        let ext = (bridge_r_px * 0.45).min(2.5);
        let (ux, uy) = if dist > 1e-3 {
            ((x2 - x1) / dist, (y2 - y1) / dist)
        } else {
            (0.0, 0.0)
        };
        let q1 = (x1 - ux * ext, y1 - uy * ext);
        let q2 = (x2 + ux * ext, y2 + uy * ext);
        draw_capsule(&mut work, w, work_h, q1, q2, bridge_r_px);
        bridge_capsules_px.push((
            [q1.0 as f64, (work_h as f64) - q1.1 as f64],
            [q2.0 as f64, (work_h as f64) - q2.1 as f64],
            bridge_r_px as f64,
        ));
        connected_pts.extend(cpts);
        if connected_pts.len() > 1800 {
            connected_pts = subsample_pts(&connected_pts, 1400);
        }
    }

    // Сохраняем маску удалённых островков для красной подсветки в предосмотре
    let mut removed_mask = vec![0u8; w * work_h];
    if !removed_ids.is_empty() {
        let mut is_rem = vec![false; sizes.len() + 1];
        for id in &removed_ids {
            is_rem[*id as usize] = true;
        }
        for i in 0..w * work_h {
            let l = labels[i] as usize;
            if l > 0 && is_rem[l] {
                work[i] = 0;
                removed_mask[i] = 1;
            }
        }
    }
    let removed_count = removed_ids.len();

    // 6. Нижняя опорная зона и планка в паз подставки
    let mut min_x = w;
    let mut max_x = 0usize;
    let mut min_y = work_h;
    let mut max_y = 0usize;
    for y in 0..work_h {
        for x in 0..w {
            if work[y * w + x] != 0 {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    if max_y <= min_y || max_x <= min_x {
        return Err("Силуэт оказался пустым после фильтрации.".into());
    }

    let fig_h_px = (max_y - min_y + 1) as f32;
    let band_top = max_y.saturating_sub(((fig_h_px * 0.08).max(4.0 * px_per_mm)) as usize);
    let mut bx0 = max_x;
    let mut bx1 = min_x;
    for y in band_top..=max_y {
        for x in min_x..=max_x {
            if work[y * w + x] != 0 {
                bx0 = bx0.min(x);
                bx1 = bx1.max(x);
            }
        }
    }
    let min_span_px = (16.0 * px_per_mm) as usize;
    if bx1 <= bx0 || (bx1 - bx0) < min_span_px {
        let bc = (bx0 + bx1) / 2;
        bx0 = bc.saturating_sub(min_span_px / 2);
        bx1 = (bc + min_span_px / 2).min(w - 1);
    }

    let mut bottom_bar_px: Option<[f64; 4]> = None;
    if params.stand_mode != "none" && params.add_bottom_bar {
        let bar_h_px = ((params.slot_depth_mm - 0.5).clamp(2.5, 5.5) * px_per_mm).round() as usize;
        let overlap_px = (2.0 * px_per_mm).round().max(2.0) as usize;
        let y_top = max_y.saturating_sub(overlap_px);
        let y_bot = (max_y + bar_h_px).min(work_h - 1);
        for y in y_top..=y_bot {
            for x in bx0..=bx1 {
                work[y * w + x] = 1;
            }
        }
        bottom_bar_px = Some([
            bx0 as f64,
            (work_h as f64) - y_bot as f64,
            bx1 as f64,
            (work_h as f64) - max_y as f64,
        ]);
    }

    // 7. ФИНАЛЬНАЯ ГАРАНТИЯ: строго 1 связное тело + заливка микро-дыр + без диагональных касаний
    let (flabels, fsizes) = label_components(&work, w, work_h, 1, false);
    if let Some((best_idx, _)) = fsizes.iter().enumerate().max_by_key(|&(_, sz)| *sz) {
        let best_id = (best_idx + 1) as u32;
        for i in 0..w * work_h {
            work[i] = u8::from(flabels[i] == best_id);
        }
    }

    // Заливка микро-отверстий (фон 8-связный)
    let min_hole_px = (params.min_hole_area_mm2 * px2_per_mm2) as usize;
    if min_hole_px > 0 {
        let (bg_lbl, bg_sz) = label_components(&work, w, work_h, 0, true);
        let mut touches_border = vec![false; bg_sz.len() + 1];
        for x in 0..w {
            touches_border[bg_lbl[x] as usize] = true;
            touches_border[bg_lbl[(work_h - 1) * w + x] as usize] = true;
        }
        for y in 0..work_h {
            touches_border[bg_lbl[y * w] as usize] = true;
            touches_border[bg_lbl[y * w + (w - 1)] as usize] = true;
        }
        for i in 0..w * work_h {
            let l = bg_lbl[i] as usize;
            if l > 0 && !touches_border[l] && bg_sz[l - 1] < min_hole_px {
                work[i] = 1;
            }
        }
    }

    // Устранение 2x2 шахматных касаний
    for y in 0..work_h.saturating_sub(1) {
        for x in 0..w.saturating_sub(1) {
            let a = work[y * w + x];
            let b = work[y * w + x + 1];
            let c = work[(y + 1) * w + x];
            let d = work[(y + 1) * w + x + 1];
            if a == 1 && d == 1 && b == 0 && c == 0 {
                work[y * w + x + 1] = 1;
            } else if b == 1 && c == 1 && a == 0 && d == 0 {
                work[y * w + x] = 1;
            }
        }
    }

    // 8. Извлечение гладких замкнутых контуров (Marching Squares)
    let raw_rings = marching_squares_rings(&work, w, work_h);
    let mut outers = Vec::new();
    let mut holes_raw = Vec::new();
    for r in raw_rings {
        let sa = signed_area(&r);
        if sa > 1.0 {
            outers.push((sa, r));
        } else if sa < -1.0 {
            holes_raw.push((-sa, r));
        }
    }
    if outers.is_empty() {
        return Err("Не удалось извлечь внешний замкнутый контур.".into());
    }
    outers.sort_by(|a, b| b.0.total_cmp(&a.0));
    let outer_px = outers.remove(0).1;

    // Оставляем только отверстия, лежащие внутри главного контура
    let holes_px: Vec<Vec<[f64; 2]>> = holes_raw
        .into_iter()
        .filter(|(_, r)| point_in_ring(r[0], &outer_px))
        .map(|(_, r)| r)
        .collect();

    // 9. Масштабирование к точному размеру target_height_mm, центрирование X=0, низ Y=0
    let mut r_minx = f64::INFINITY;
    let mut r_maxx = f64::NEG_INFINITY;
    let mut r_miny = f64::INFINITY;
    let mut r_maxy = f64::NEG_INFINITY;
    for &[x, y] in &outer_px {
        r_minx = r_minx.min(x);
        r_maxx = r_maxx.max(x);
        r_miny = r_miny.min(y);
        r_maxy = r_maxy.max(y);
    }
    let scale_mm = params.target_height_mm as f64 / (r_maxy - r_miny).max(1.0);
    let cx_px = 0.5 * (r_minx + r_maxx);
    let to_mm = |p: [f64; 2]| -> [f64; 2] {
        [(p[0] - cx_px) * scale_mm, (p[1] - r_miny) * scale_mm]
    };

    let outer_mm_raw: Vec<[f64; 2]> = outer_px.iter().map(|&p| to_mm(p)).collect();
    let holes_mm_raw: Vec<Vec<[f64; 2]>> = holes_px
        .iter()
        .map(|r| r.iter().map(|&p| to_mm(p)).collect::<Vec<_>>())
        .filter(|r| (-signed_area(r)) >= params.min_hole_area_mm2 as f64)
        .collect();

    // 10. Безопасное упрощение контуров без самопересечений
    let (outer, holes) = safe_simplify(&outer_mm_raw, &holes_mm_raw, params.simplify_tol_mm as f64);

    // Визуализация мостов и удалённых островков в мм
    let bridges: Vec<Vec<[f64; 2]>> = bridge_capsules_px
        .into_iter()
        .map(|(a, b, r)| capsule_polygon(to_mm(a), to_mm(b), r * scale_mm))
        .collect();

    let removed_rings_raw = if removed_count > 0 {
        marching_squares_rings(&removed_mask, w, work_h)
            .into_iter()
            .filter(|r| signed_area(r) > 0.5)
            .map(|r| {
                let mm: Vec<[f64; 2]> = r.iter().map(|&p| to_mm(p)).collect();
                simplify_closed_ring(&mm, 0.35)
            })
            .collect()
    } else {
        Vec::new()
    };

    let bottom_bar = bottom_bar_px.map(|[x0, y0, x1, y1]| {
        let a = to_mm([x0, y0]);
        let b = to_mm([x1, y1]);
        [a[0], a[1], b[0], b[1]]
    });

    let sx_a = (bx0 as f64 - cx_px) * scale_mm;
    let sx_b = (bx1 as f64 - cx_px) * scale_mm;
    let stand_span_x = [sx_a, sx_b];
    let stand_box_x = [
        sx_a - params.stand_margin_mm as f64,
        sx_b + params.stand_margin_mm as f64,
    ];

    // Границы исходного изображения в мм-координатах
    let img_y0_px = extra_bot as f64;
    let img_y1_px = work_h as f64;
    let img_a = to_mm([0.0, img_y0_px]);
    let img_b = to_mm([w as f64, img_y1_px]);
    let img_bounds_mm = [img_a[0], img_a[1], img_b[0], img_b[1]];

    let (mut f_minx, mut f_maxx, mut f_miny, mut f_maxy) =
        (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    for &[x, y] in &outer {
        f_minx = f_minx.min(x);
        f_maxx = f_maxx.max(x);
        f_miny = f_miny.min(y);
        f_maxy = f_maxy.max(y);
    }
    let vertex_count = outer.len() + holes.iter().map(|r| r.len()).sum::<usize>();

    let stats = ProcessStats {
        connected_components: 1,
        open_contours: 0,
        islands_in_holes_remaining: 0,
        bridges_added: bridges.len(),
        islands_removed: removed_count,
        holes_count: holes.len(),
        width_mm: ((f_maxx - f_minx) * 10.0).round() / 10.0,
        height_mm: ((f_maxy - f_miny) * 10.0).round() / 10.0,
        thickness_mm: ((params.thickness_mm as f64) * 100.0).round() / 100.0,
        vertex_count,
    };

    Ok(StencilModel {
        outer,
        holes,
        bridges,
        removed: removed_rings_raw,
        bottom_bar,
        stand_span_x,
        stand_box_x,
        img_bounds_mm,
        bounds_mm: [f_minx, f_miny, f_maxx, f_maxy],
        stats,
    })
}
