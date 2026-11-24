use crate::processor::{StencilModel, StencilParams};

pub struct Mesh3D {
    pub verts: Vec<[f32; 3]>,
    pub tris: Vec<[u32; 3]>,
}

fn push_extruded_poly(
    mesh: &mut Mesh3D,
    outer: &[[f64; 2]],
    holes: &[Vec<[f64; 2]>],
    thickness: f64,
    upright_z: Option<f64>,
) {
    let mut flat = Vec::with_capacity((outer.len() + holes.iter().map(|h| h.len()).sum::<usize>()) * 2);
    let mut hole_idx = Vec::with_capacity(holes.len());
    for &[x, y] in outer {
        flat.push(x);
        flat.push(y);
    }
    for h in holes {
        hole_idx.push(flat.len() / 2);
        for &[x, y] in h {
            flat.push(x);
            flat.push(y);
        }
    }

    let n_pts = flat.len() / 2;
    let base_b = mesh.verts.len() as u32;
    let base_t = base_b + n_pts as u32;

    let xform = |x: f64, y: f64, z: f64| -> [f32; 3] {
        if let Some(z0) = upright_z {
            [x as f32, (0.5 * thickness - z) as f32, (y + z0) as f32]
        } else {
            [x as f32, y as f32, z as f32]
        }
    };

    for i in 0..n_pts {
        mesh.verts.push(xform(flat[i * 2], flat[i * 2 + 1], 0.0));
    }
    for i in 0..n_pts {
        mesh.verts.push(xform(flat[i * 2], flat[i * 2 + 1], thickness));
    }

    if let Ok(indices) = earcutr::earcut(&flat, &hole_idx, 2) {
        for tri in indices.chunks_exact(3) {
            let (i0, i1, i2) = (tri[0], tri[1], tri[2]);
            let ax = flat[i1 * 2] - flat[i0 * 2];
            let ay = flat[i1 * 2 + 1] - flat[i0 * 2 + 1];
            let bx = flat[i2 * 2] - flat[i0 * 2];
            let by = flat[i2 * 2 + 1] - flat[i0 * 2 + 1];
            let (a, b, c) = if ax * by - ay * bx >= 0.0 {
                (i0 as u32, i1 as u32, i2 as u32)
            } else {
                (i0 as u32, i2 as u32, i1 as u32)
            };
            // Верх (+Z) и низ (-Z)
            mesh.tris.push([base_t + a, base_t + b, base_t + c]);
            mesh.tris.push([base_b + a, base_b + c, base_b + b]);
        }
    }

    // Боковые стенки
    let mut offset = 0u32;
    for ring_len in std::iter::once(outer.len()).chain(holes.iter().map(|h| h.len())) {
        let m = ring_len as u32;
        for i in 0..m {
            let j = (i + 1) % m;
            let bi = base_b + offset + i;
            let bj = base_b + offset + j;
            let ti = base_t + offset + i;
            let tj = base_t + offset + j;
            mesh.tris.push([bi, bj, tj]);
            mesh.tris.push([bi, tj, ti]);
        }
        offset += m;
    }
}

fn push_box(mesh: &mut Mesh3D, x0: f64, y0: f64, z0: f64, x1: f64, y1: f64, z1: f64) {
    let b = mesh.verts.len() as u32;
    let pts = [
        [x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0],
        [x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1],
    ];
    for p in pts {
        mesh.verts.push([p[0] as f32, p[1] as f32, p[2] as f32]);
    }
    let quads = [
        [0, 3, 2, 1], // низ
        [4, 5, 6, 7], // верх
        [0, 1, 5, 4], // перед
        [1, 2, 6, 5], // право
        [2, 3, 7, 6], // зад
        [3, 0, 4, 7], // лево
    ];
    for q in quads {
        mesh.tris.push([b + q[0], b + q[1], b + q[2]]);
        mesh.tris.push([b + q[0], b + q[2], b + q[3]]);
    }
}

fn push_stepped_stand_mesh(
    mesh: &mut Mesh3D,
    x0: f64,
    x1: f64,
    yc: f64,
    depth: f64,
    total_h: f64,
    slot: Option<([f64; 2], f64, f64)>,
) {
    let w = (x1 - x0).max(24.0);
    let cx = 0.5 * (x0 + x1);
    let hw = 0.5 * w;
    let hd = 0.5 * depth.max(12.0);
    let h1 = total_h * 0.44;
    let inset = 3.5_f64.min(hd * 0.25).min(hw * 0.12);

    // Нижняя ступень
    push_box(mesh, cx - hw, yc - hd, 0.0, cx + hw, yc + hd, h1);

    let ux0 = cx - (hw - inset);
    let ux1 = cx + (hw - inset);
    let uy0 = yc - (hd - inset);
    let uy1 = yc + (hd - inset);

    if let Some(([raw_sx0, raw_sx1], slot_w, slot_depth)) = slot {
        let eff_d = slot_depth.clamp(1.5, total_h - 1.5);
        let z_floor = total_h - eff_d;
        let sx0 = raw_sx0.max(ux0 + 1.5);
        let sx1 = raw_sx1.min(ux1 - 1.5);
        let hsw = (0.5 * slot_w).min(hd - inset - 1.5).max(0.6);

        if z_floor > h1 {
            push_box(mesh, ux0, uy0, h1, ux1, uy1, z_floor);
        }
        let z_wall_lo = z_floor.min(h1);
        // 4 стенки вокруг паза
        push_box(mesh, ux0, uy0, z_wall_lo, ux1, yc - hsw, total_h);
        push_box(mesh, ux0, yc + hsw, z_wall_lo, ux1, uy1, total_h);
        push_box(mesh, ux0, yc - hsw, z_wall_lo, sx0, yc + hsw, total_h);
        push_box(mesh, sx1, yc - hsw, z_wall_lo, ux1, yc + hsw, total_h);
    } else {
        push_box(mesh, ux0, uy0, h1, ux1, uy1, total_h);
    }
}

pub fn build_mesh3d(model: &StencilModel, params: &StencilParams) -> Mesh3D {
    let mut mesh = Mesh3D {
        verts: Vec::new(),
        tris: Vec::new(),
    };
    let t = params.thickness_mm as f64;
    match params.stand_mode.as_str() {
        "none" => {
            push_extruded_poly(&mut mesh, &model.outer, &model.holes, t, None);
        }
        "assembled" => {
            let sh = params.stand_height_mm as f64;
            let embed = (params.slot_depth_mm as f64).min(sh * 0.65);
            push_extruded_poly(&mut mesh, &model.outer, &model.holes, t, Some(sh - embed));
            push_stepped_stand_mesh(
                &mut mesh,
                model.stand_box_x[0],
                model.stand_box_x[1],
                0.0,
                params.stand_depth_mm as f64,
                sh,
                None,
            );
        }
        _ => {
            push_extruded_poly(&mut mesh, &model.outer, &model.holes, t, None);
            let y_stand = -(params.stand_depth_mm as f64 * 0.5 + 14.0);
            let clear = params.slot_clearance_mm as f64;
            push_stepped_stand_mesh(
                &mut mesh,
                model.stand_box_x[0],
                model.stand_box_x[1],
                y_stand,
                params.stand_depth_mm as f64,
                params.stand_height_mm as f64,
                Some((
                    [model.stand_span_x[0] - clear, model.stand_span_x[1] + clear],
                    t + clear,
                    params.slot_depth_mm as f64,
                )),
            );
        }
    }
    mesh
}

pub fn pack_mesh_bytes(mesh: &Mesh3D) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + mesh.verts.len() * 12 + mesh.tris.len() * 12);
    out.extend_from_slice(&(mesh.verts.len() as u32).to_le_bytes());
    for v in &mesh.verts {
        out.extend_from_slice(&v[0].to_le_bytes());
        out.extend_from_slice(&v[1].to_le_bytes());
        out.extend_from_slice(&v[2].to_le_bytes());
    }
    out.extend_from_slice(&(mesh.tris.len() as u32).to_le_bytes());
    for t in &mesh.tris {
        out.extend_from_slice(&t[0].to_le_bytes());
        out.extend_from_slice(&t[1].to_le_bytes());
        out.extend_from_slice(&t[2].to_le_bytes());
    }
    out
}

pub fn generate_binary_stl(mesh: &Mesh3D) -> Vec<u8> {
    let mut out = Vec::with_capacity(84 + mesh.tris.len() * 50);
    let mut header = [0u8; 80];
    let tag = b"Stencil3D Rust Binary STL";
    header[..tag.len()].copy_from_slice(tag);
    out.extend_from_slice(&header);
    out.extend_from_slice(&(mesh.tris.len() as u32).to_le_bytes());
    for &[i0, i1, i2] in &mesh.tris {
        let a = mesh.verts[i0 as usize];
        let b = mesh.verts[i1 as usize];
        let c = mesh.verts[i2 as usize];
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let mut n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-8);
        n[0] /= len;
        n[1] /= len;
        n[2] /= len;
        for f in [n[0], n[1], n[2], a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2]] {
            out.extend_from_slice(&f.to_le_bytes());
        }
        out.extend_from_slice(&0u16.to_le_bytes());
    }
    out
}
