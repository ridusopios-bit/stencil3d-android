use crate::processor::{StencilModel, StencilParams};
use std::fmt::Write;

/// Генератор 100% валидного ISO 10303-21 / STEP AP214 (MANIFOLD_SOLID_BREP, миллиметры)
/// на чистом Rust — без OpenCASCADE и C++ библиотек.
struct StepBuilder {
    next_id: usize,
    buf: String,
}

impl StepBuilder {
    fn new() -> Self {
        let mut s = Self {
            next_id: 18,
            buf: String::with_capacity(256 * 1024),
        };
        s.buf.push_str(
            "ISO-10303-21;\n\
             HEADER;\n\
             FILE_DESCRIPTION(('Stencil3D STEP AP214 Model'),'2;1');\n\
             FILE_NAME('stencil3d.step','2025-01-01T00:00:00',('Stencil3D'),('Stencil3D'),'Stencil3D Rust B-Rep','Stencil3D','');\n\
             FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));\n\
             ENDSEC;\n\
             DATA;\n\
             #1=APPLICATION_CONTEXT('automotive design');\n\
             #2=APPLICATION_PROTOCOL_DEFINITION('international standard','automotive_design',2000,#1);\n\
             #3=PRODUCT_CONTEXT('',#1,'mechanical');\n\
             #4=PRODUCT('Stencil3D','Stencil3D','',(#3));\n\
             #5=PRODUCT_DEFINITION_FORMATION('','',#4);\n\
             #6=PRODUCT_DEFINITION_CONTEXT('part definition',#1,'design');\n\
             #7=PRODUCT_DEFINITION('design','',#5,#6);\n\
             #8=PRODUCT_DEFINITION_SHAPE('','',#7);\n\
             #9=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));\n\
             #10=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.));\n\
             #11=(NAMED_UNIT(*)SI_UNIT($,.STERADIAN.)SOLID_ANGLE_UNIT());\n\
             #12=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-05),#9,'distance_accuracy_value','confusion accuracy');\n\
             #13=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#12))GLOBAL_UNIT_ASSIGNED_CONTEXT((#9,#10,#11))REPRESENTATION_CONTEXT('Context #1','3D Context with UNIT and UNCERTAINTY'));\n\
             #14=CARTESIAN_POINT('',(0.,0.,0.));\n\
             #15=DIRECTION('',(0.,0.,1.));\n\
             #16=DIRECTION('',(1.,0.,0.));\n\
             #17=AXIS2_PLACEMENT_3D('',#14,#15,#16);\n",
        );
        s
    }

    fn emit(&mut self, body: &str) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let _ = writeln!(self.buf, "#{id}={body};");
        id
    }

    fn pt(&mut self, p: [f64; 3]) -> usize {
        self.emit(&format!(
            "CARTESIAN_POINT('',({},{},{}))",
            fmt_r(p[0]),
            fmt_r(p[1]),
            fmt_r(p[2])
        ))
    }

    fn dir(&mut self, d: [f64; 3]) -> usize {
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-12);
        self.emit(&format!(
            "DIRECTION('',({},{},{}))",
            fmt_r(d[0] / len),
            fmt_r(d[1] / len),
            fmt_r(d[2] / len)
        ))
    }

    fn vertex(&mut self, p: [f64; 3]) -> (usize, usize) {
        let pid = self.pt(p);
        let vid = self.emit(&format!("VERTEX_POINT('',#{pid})"));
        (pid, vid)
    }

    fn edge_curve(&mut self, p1: [f64; 3], p2: [f64; 3], p1_id: usize, v1: usize, v2: usize) -> usize {
        let d = [p2[0] - p1[0], p2[1] - p1[1], p2[2] - p1[2]];
        let did = self.dir(d);
        let vec_id = self.emit(&format!("VECTOR('',#{did},1.)"));
        let line_id = self.emit(&format!("LINE('',#{p1_id},#{vec_id})"));
        self.emit(&format!("EDGE_CURVE('',#{v1},#{v2},#{line_id},.T.)"))
    }

    fn oriented_edge(&mut self, ec: usize, forward: bool) -> usize {
        let flag = if forward { ".T." } else { ".F." };
        self.emit(&format!("ORIENTED_EDGE('',*,*,#{ec},{flag})"))
    }

    fn edge_loop(&mut self, edges: &[usize]) -> usize {
        let list = id_list(edges);
        self.emit(&format!("EDGE_LOOP('',({list}))"))
    }

    fn plane(&mut self, origin: [f64; 3], normal: [f64; 3], ref_dir: [f64; 3]) -> usize {
        let o = self.pt(origin);
        let n = self.dir(normal);
        let r = self.dir(ref_dir);
        let ax = self.emit(&format!("AXIS2_PLACEMENT_3D('',#{o},#{n},#{r})"));
        self.emit(&format!("PLANE('',#{ax})"))
    }

    fn advanced_face(&mut self, outer_loop: usize, hole_loops: &[usize], plane_id: usize) -> usize {
        let mut bounds = Vec::with_capacity(1 + hole_loops.len());
        bounds.push(self.emit(&format!("FACE_OUTER_BOUND('',#{outer_loop},.T.)")));
        for &hl in hole_loops {
            bounds.push(self.emit(&format!("FACE_BOUND('',#{hl},.T.)")));
        }
        let b_list = id_list(&bounds);
        self.emit(&format!("ADVANCED_FACE('',({b_list}),#{plane_id},.T.)"))
    }

    /// Строит твёрдое тело выдавливания 2D-полигона с отверстиями (с опциональным поворотом стоя).
    fn add_extruded_polygon(
        &mut self,
        name: &str,
        outer: &[[f64; 2]],
        holes: &[Vec<[f64; 2]>],
        thickness: f64,
        upright_z_offset: Option<f64>,
    ) -> usize {
        let xform_pt = |p: [f64; 3]| -> [f64; 3] {
            if let Some(z0) = upright_z_offset {
                // Чистый поворот +90° вокруг оси X (det = +1, ориентация сохраняется!)
                [p[0], 0.5 * thickness - p[2], p[1] + z0]
            } else {
                p
            }
        };
        let xform_dir = |d: [f64; 3]| -> [f64; 3] {
            if upright_z_offset.is_some() {
                [d[0], -d[2], d[1]]
            } else {
                d
            }
        };

        let mut all_rings: Vec<&[[f64; 2]]> = Vec::with_capacity(1 + holes.len());
        all_rings.push(outer);
        for h in holes {
            all_rings.push(h.as_slice());
        }

        let mut faces = Vec::new();
        let mut bot_loops = Vec::with_capacity(all_rings.len());
        let mut top_loops = Vec::with_capacity(all_rings.len());

        for ring in &all_rings {
            let n = ring.len();
            let mut b_pts = Vec::with_capacity(n);
            let mut t_pts = Vec::with_capacity(n);
            let mut b_pids = Vec::with_capacity(n);
            let mut b_vids = Vec::with_capacity(n);
            let mut t_pids = Vec::with_capacity(n);
            let mut t_vids = Vec::with_capacity(n);

            for &[x, y] in *ring {
                let bp = xform_pt([x, y, 0.0]);
                let tp = xform_pt([x, y, thickness]);
                let (bpid, bvid) = self.vertex(bp);
                let (tpid, tvid) = self.vertex(tp);
                b_pts.push(bp);
                t_pts.push(tp);
                b_pids.push(bpid);
                b_vids.push(bvid);
                t_pids.push(tpid);
                t_vids.push(tvid);
            }

            let mut e_bot = Vec::with_capacity(n);
            let mut e_top = Vec::with_capacity(n);
            let mut e_vert = Vec::with_capacity(n);

            for i in 0..n {
                let j = (i + 1) % n;
                e_bot.push(self.edge_curve(b_pts[i], b_pts[j], b_pids[i], b_vids[i], b_vids[j]));
                e_top.push(self.edge_curve(t_pts[i], t_pts[j], t_pids[i], t_vids[i], t_vids[j]));
                e_vert.push(self.edge_curve(b_pts[i], t_pts[i], b_pids[i], b_vids[i], t_vids[i]));
            }

            // Нижний цикл: в обратном порядке и .F. (снаружи -Z это CCW для внешнего и CW для дыр)
            let mut bot_oe = Vec::with_capacity(n);
            for i in (0..n).rev() {
                bot_oe.push(self.oriented_edge(e_bot[i], false));
            }
            bot_loops.push(self.edge_loop(&bot_oe));

            // Верхний цикл: в прямом порядке и .T.
            let mut top_oe = Vec::with_capacity(n);
            for i in 0..n {
                top_oe.push(self.oriented_edge(e_top[i], true));
            }
            top_loops.push(self.edge_loop(&top_oe));

            // Боковые вертикальные грани: B_i -> B_j -> T_j -> T_i
            for i in 0..n {
                let j = (i + 1) % n;
                let oe1 = self.oriented_edge(e_bot[i], true);
                let oe2 = self.oriented_edge(e_vert[j], true);
                let oe3 = self.oriented_edge(e_top[i], false);
                let oe4 = self.oriented_edge(e_vert[i], false);
                let loop_id = self.edge_loop(&[oe1, oe2, oe3, oe4]);

                let dx = ring[j][0] - ring[i][0];
                let dy = ring[j][1] - ring[i][1];
                let normal = xform_dir([dy, -dx, 0.0]);
                let ref_d = xform_dir([0.0, 0.0, 1.0]);
                let pln = self.plane(b_pts[i], normal, ref_d);
                faces.push(self.advanced_face(loop_id, &[], pln));
            }
        }

        // Нижняя грань
        let bot_pln = self.plane(
            xform_pt([0.0, 0.0, 0.0]),
            xform_dir([0.0, 0.0, -1.0]),
            xform_dir([1.0, 0.0, 0.0]),
        );
        faces.push(self.advanced_face(bot_loops[0], &bot_loops[1..], bot_pln));

        // Верхняя грань
        let top_pln = self.plane(
            xform_pt([0.0, 0.0, thickness]),
            xform_dir([0.0, 0.0, 1.0]),
            xform_dir([1.0, 0.0, 0.0]),
        );
        faces.push(self.advanced_face(top_loops[0], &top_loops[1..], top_pln));

        let f_list = id_list(&faces);
        let shell = self.emit(&format!("CLOSED_SHELL('',({f_list}))"));
        self.emit(&format!("MANIFOLD_SOLID_BREP('{name}',#{shell})"))
    }

    /// Строит точную 16-гранную 2-ступенчатую подставку с глухим пазом (или 11-гранную без паза).
    fn add_stepped_stand(
        &mut self,
        name: &str,
        x0: f64,
        x1: f64,
        y_center: f64,
        depth: f64,
        total_h: f64,
        slot: Option<([f64; 2], f64, f64)>, // (slot_x[2], slot_w, slot_depth)
    ) -> usize {
        let w = (x1 - x0).max(24.0);
        let cx = 0.5 * (x0 + x1);
        let hw = 0.5 * w;
        let hd = 0.5 * depth.max(12.0);
        let h1 = total_h * 0.44;
        let inset = 3.5_f64.min(hd * 0.25).min(hw * 0.12);

        let rect_ccw = |rx0: f64, ry0: f64, rx1: f64, ry1: f64| -> [[f64; 2]; 4] {
            [[rx0, ry0], [rx1, ry0], [rx1, ry1], [rx0, ry1]]
        };
        let rect_cw = |rx0: f64, ry0: f64, rx1: f64, ry1: f64| -> [[f64; 2]; 4] {
            [[rx0, ry0], [rx0, ry1], [rx1, ry1], [rx1, ry0]]
        };

        let make_ring = |this: &mut Self, pts2: [[f64; 2]; 4], z: f64| -> ([ [f64; 3]; 4 ], [usize; 4], [usize; 4], [usize; 4]) {
            let mut p3 = [[0.0; 3]; 4];
            let mut pids = [0usize; 4];
            let mut vids = [0usize; 4];
            for i in 0..4 {
                p3[i] = [pts2[i][0], pts2[i][1], z];
                let (pid, vid) = this.vertex(p3[i]);
                pids[i] = pid;
                vids[i] = vid;
            }
            let mut edges = [0usize; 4];
            for i in 0..4 {
                let j = (i + 1) % 4;
                edges[i] = this.edge_curve(p3[i], p3[j], pids[i], vids[i], vids[j]);
            }
            (p3, pids, vids, edges)
        };

        let add_tube_walls = |this: &mut Self,
                              p_lo: [[f64; 3]; 4],
                              pid_lo: [usize; 4],
                              vid_lo: [usize; 4],
                              e_lo: [usize; 4],
                              p_hi: [[f64; 3]; 4],
                              vid_hi: [usize; 4],
                              e_hi: [usize; 4],
                              faces: &mut Vec<usize>| {
            let mut e_v = [0usize; 4];
            for i in 0..4 {
                e_v[i] = this.edge_curve(p_lo[i], p_hi[i], pid_lo[i], vid_lo[i], vid_hi[i]);
            }
            for i in 0..4 {
                let j = (i + 1) % 4;
                let oe1 = this.oriented_edge(e_lo[i], true);
                let oe2 = this.oriented_edge(e_v[j], true);
                let oe3 = this.oriented_edge(e_hi[i], false);
                let oe4 = this.oriented_edge(e_v[i], false);
                let lp = this.edge_loop(&[oe1, oe2, oe3, oe4]);
                let dx = p_lo[j][0] - p_lo[i][0];
                let dy = p_lo[j][1] - p_lo[i][1];
                let pln = this.plane(p_lo[i], [dy, -dx, 0.0], [0.0, 0.0, 1.0]);
                faces.push(this.advanced_face(lp, &[], pln));
            }
        };

        let loop_fwd = |this: &mut Self, e: [usize; 4]| -> usize {
            let oe = [
                this.oriented_edge(e[0], true),
                this.oriented_edge(e[1], true),
                this.oriented_edge(e[2], true),
                this.oriented_edge(e[3], true),
            ];
            this.edge_loop(&oe)
        };
        let loop_rev = |this: &mut Self, e: [usize; 4]| -> usize {
            let oe = [
                this.oriented_edge(e[3], false),
                this.oriented_edge(e[2], false),
                this.oriented_edge(e[1], false),
                this.oriented_edge(e[0], false),
            ];
            this.edge_loop(&oe)
        };

        let r_base = rect_ccw(cx - hw, y_center - hd, cx + hw, y_center + hd);
        let r_top = rect_ccw(
            cx - (hw - inset),
            y_center - (hd - inset),
            cx + (hw - inset),
            y_center + (hd - inset),
        );

        let (pb0, pid_b0, vid_b0, e_b0) = make_ring(self, r_base, 0.0);
        let (pb1, _, vid_b1, e_b1) = make_ring(self, r_base, h1);
        let (pt1, pid_t1, vid_t1, e_t1) = make_ring(self, r_top, h1);
        let (pt2, _, vid_t2, e_t2) = make_ring(self, r_top, total_h);

        let mut faces = Vec::with_capacity(16);

        // 1) Нижняя грань (z=0, нормаль -Z)
        let lp_bot = loop_rev(self, e_b0);
        let pln_bot = self.plane([cx, y_center, 0.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]);
        faces.push(self.advanced_face(lp_bot, &[], pln_bot));

        // 2) Боковые стены нижней ступени
        add_tube_walls(self, pb0, pid_b0, vid_b0, e_b0, pb1, vid_b1, e_b1, &mut faces);

        // 3) Горизонтальная ступенька (z=h1, внешняя B1, отверстие T1)
        let lp_ledge_out = loop_fwd(self, e_b1);
        let lp_ledge_hole = loop_rev(self, e_t1);
        let pln_h1 = self.plane([cx, y_center, h1], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
        faces.push(self.advanced_face(lp_ledge_out, &[lp_ledge_hole], pln_h1));

        // 4) Боковые стены верхней ступени
        add_tube_walls(self, pt1, pid_t1, vid_t1, e_t1, pt2, vid_t2, e_t2, &mut faces);

        // 5) Верхняя грань + паз (если включён)
        let lp_top_out = loop_fwd(self, e_t2);
        let pln_top = self.plane([cx, y_center, total_h], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);

        if let Some(([raw_sx0, raw_sx1], slot_w, slot_depth)) = slot {
            let eff_depth = slot_depth.clamp(1.5, total_h - 1.5);
            let h_slot_bot = total_h - eff_depth;
            let sx0 = raw_sx0.max(cx - (hw - inset) + 1.5);
            let sx1 = raw_sx1.min(cx + (hw - inset) - 1.5);
            let (sx0, sx1) = if sx1 - sx0 < 6.0 {
                (cx - (hw - inset - 2.0), cx + (hw - inset - 2.0))
            } else {
                (sx0, sx1)
            };
            let hsw = (0.5 * slot_w).min(hd - inset - 1.5).max(0.6);

            let r_slot = rect_cw(sx0, y_center - hsw, sx1, y_center + hsw);
            let (ps0, pid_s0, vid_s0, e_s0) = make_ring(self, r_slot, h_slot_bot);
            let (ps2, _, vid_s2, e_s2) = make_ring(self, r_slot, total_h);

            // Верхняя кромка с отверстием S2 (S2 уже CW -> forward)
            let lp_slot_top = loop_fwd(self, e_s2);
            faces.push(self.advanced_face(lp_top_out, &[lp_slot_top], pln_top));

            // 4 внутренние стенки паза
            add_tube_walls(self, ps0, pid_s0, vid_s0, e_s0, ps2, vid_s2, e_s2, &mut faces);

            // Дно паза (z=h_slot_bot, нормаль +Z -> S0 reversed = CCW!)
            let lp_slot_floor = loop_rev(self, e_s0);
            let pln_floor = self.plane([cx, y_center, h_slot_bot], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
            faces.push(self.advanced_face(lp_slot_floor, &[], pln_floor));
        } else {
            faces.push(self.advanced_face(lp_top_out, &[], pln_top));
        }

        let f_list = id_list(&faces);
        let shell = self.emit(&format!("CLOSED_SHELL('',({f_list}))"));
        self.emit(&format!("MANIFOLD_SOLID_BREP('{name}',#{shell})"))
    }

    fn finish(mut self, solids: &[usize]) -> String {
        let mut items = vec![17usize];
        items.extend_from_slice(solids);
        let list = id_list(&items);
        let rep = self.emit(&format!(
            "ADVANCED_BREP_SHAPE_REPRESENTATION('Stencil3D',({list}),#13)"
        ));
        self.emit(&format!("SHAPE_DEFINITION_REPRESENTATION(#8,#{rep})"));
        self.buf.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
        self.buf
    }
}

fn fmt_r(v: f64) -> String {
    let clean = if v.abs() < 1e-8 { 0.0 } else { v };
    let s = format!("{clean:.5}");
    s.trim_end_matches('0').to_string()
}

fn id_list(ids: &[usize]) -> String {
    let mut s = String::with_capacity(ids.len() * 6);
    for (i, id) in ids.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(s, "#{id}");
    }
    s
}

pub fn generate_step(model: &StencilModel, params: &StencilParams) -> Vec<u8> {
    let mut b = StepBuilder::new();
    let thickness = params.thickness_mm as f64;
    let mut solids = Vec::new();

    match params.stand_mode.as_str() {
        "none" => {
            solids.push(b.add_extruded_polygon("Figure", &model.outer, &model.holes, thickness, None));
        }
        "assembled" => {
            let sh = params.stand_height_mm as f64;
            let embed = (params.slot_depth_mm as f64).min(sh * 0.65);
            solids.push(b.add_extruded_polygon(
                "Figure",
                &model.outer,
                &model.holes,
                thickness,
                Some(sh - embed),
            ));
            solids.push(b.add_stepped_stand(
                "Stand",
                model.stand_box_x[0],
                model.stand_box_x[1],
                0.0,
                params.stand_depth_mm as f64,
                sh,
                None,
            ));
        }
        _ => {
            // "slotted" (по умолчанию): фигура лежит плашмя Z=0, подставка с пазом рядом
            solids.push(b.add_extruded_polygon("Figure", &model.outer, &model.holes, thickness, None));
            let y_stand = -(params.stand_depth_mm as f64 * 0.5 + 14.0);
            let clear = params.slot_clearance_mm as f64;
            solids.push(b.add_stepped_stand(
                "SlottedStand",
                model.stand_box_x[0],
                model.stand_box_x[1],
                y_stand,
                params.stand_depth_mm as f64,
                params.stand_height_mm as f64,
                Some((
                    [model.stand_span_x[0] - clear, model.stand_span_x[1] + clear],
                    thickness + clear,
                    params.slot_depth_mm as f64,
                )),
            ));
        }
    }

    b.finish(&solids).into_bytes()
}

pub fn generate_svg(model: &StencilModel) -> Vec<u8> {
    let [minx, miny, maxx, maxy] = model.bounds_mm;
    let w = (maxx - minx).max(1.0);
    let h = (maxy - miny).max(1.0);
    let mut d = String::new();
    let append_ring = |out: &mut String, ring: &[[f64; 2]]| {
        if let Some(&first) = ring.first() {
            let _ = write!(out, "M {:.3},{:.3}", first[0] - minx, maxy - first[1]);
            for &[x, y] in ring.iter().skip(1) {
                let _ = write!(out, " L {:.3},{:.3}", x - minx, maxy - y);
            }
            out.push_str(" Z ");
        }
    };
    append_ring(&mut d, &model.outer);
    for hole in &model.holes {
        append_ring(&mut d, hole);
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.2}mm\" height=\"{h:.2}mm\" viewBox=\"0 0 {w:.3} {h:.3}\">\n\
           <path fill=\"#111111\" fill-rule=\"evenodd\" d=\"{d}\"/>\n\
         </svg>\n"
    )
    .into_bytes()
}
