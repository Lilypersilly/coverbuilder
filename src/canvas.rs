use crate::models::{
    point_in_polygon, rotate_vec, BookCover, CompositionOverlay, CoverShape, InstalledFont,
    ShapeType, TextAlign, TextBlock,
};
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragTarget {
    Shape(usize),
    Text(usize),
}

/// Draws any convex or concave polygon in `egui` using ear-clipping triangulation
/// AND adds a smooth anti-aliased feathered edge when `anti_alias` is enabled!
pub fn draw_arbitrary_polygon(
    painter: &egui::Painter,
    pts: &[egui::Pos2],
    colour: egui::Color32,
    anti_alias: bool,
) {
    if pts.len() < 3 {
        return;
    }

    let mut area = 0.0;
    for i in 0..pts.len() {
        let p1 = pts[i];
        let p2 = pts[(i + 1) % pts.len()];
        area += p1.x * p2.y - p2.x * p1.y;
    }

    let mut indices: Vec<usize> = (0..pts.len()).collect();
    if area < 0.0 {
        indices.reverse();
    }

    let mut mesh = egui::Mesh::default();
    for &p in pts {
        mesh.colored_vertex(p, colour);
    }

    let cross = |a: egui::Pos2, b: egui::Pos2, c: egui::Pos2| -> f32 {
        (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
    };

    let pt_in_tri = |p: egui::Pos2, a: egui::Pos2, b: egui::Pos2, c: egui::Pos2| -> bool {
        let d1 = cross(p, a, b);
        let d2 = cross(p, b, c);
        let d3 = cross(p, c, a);
        let has_neg = (d1 < 0.0) || (d2 < 0.0) || (d3 < 0.0);
        let has_pos = (d1 > 0.0) || (d2 > 0.0) || (d3 > 0.0);
        !(has_neg && has_pos)
    };

    let mut guard = indices.len() * 3;
    while indices.len() > 2 && guard > 0 {
        guard -= 1;
        let n = indices.len();
        let mut ear_found = false;

        for i in 0..n {
            let prev = indices[(i + n - 1) % n];
            let curr = indices[i];
            let next = indices[(i + 1) % n];

            let a = pts[prev];
            let b = pts[curr];
            let c = pts[next];

            if cross(a, b, c) > 1e-4 {
                let mut contains_other = false;
                for &other_idx in &indices {
                    if other_idx != prev && other_idx != curr && other_idx != next {
                        if pt_in_tri(pts[other_idx], a, b, c) {
                            contains_other = true;
                            break;
                        }
                    }
                }
                if !contains_other {
                    mesh.add_triangle(prev as u32, curr as u32, next as u32);
                    indices.remove(i);
                    ear_found = true;
                    break;
                }
            }
        }

        if !ear_found {
            let prev = indices[0];
            let curr = indices[1];
            let next = indices[2];
            mesh.add_triangle(prev as u32, curr as u32, next as u32);
            indices.remove(1);
        }
    }

    painter.add(egui::Shape::mesh(mesh));

    // Smooth 1.25px anti-aliased feather stroke around the polygon perimeter to eliminate stair-stepping
    if anti_alias {
        painter.add(egui::Shape::closed_line(
            pts.to_vec(),
            egui::Stroke::new(1.25, colour),
        ));
    }
}

/// Draws a vertical gradient rectangle in `egui`
pub fn draw_vertical_gradient_rect(
    painter: &egui::Painter,
    rect: egui::Rect,
    top_colour: egui::Color32,
    bottom_colour: egui::Color32,
) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top_colour);
    mesh.colored_vertex(rect.right_top(), top_colour);
    mesh.colored_vertex(rect.right_bottom(), bottom_colour);
    mesh.colored_vertex(rect.left_bottom(), bottom_colour);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

/// Draws subtle vintage paper grain / noise across the cover
pub fn draw_paper_grain_overlay(
    painter: &egui::Painter,
    rect: egui::Rect,
    intensity: f32,
    zoom: f32,
) {
    if intensity <= 0.01 {
        return;
    }
    let alpha = (intensity * 36.0).clamp(1.0, 60.0) as u8;
    let step = (6.0 * zoom).clamp(3.0, 14.0);
    let cols = (rect.width() / step) as i32;
    let rows = (rect.height() / step) as i32;

    for r in 0..rows {
        for c in 0..cols {
            let hash = ((r as u32).wrapping_mul(73856093)) ^ ((c as u32).wrapping_mul(19349663));
            if hash % 5 == 0 {
                let x = rect.left() + (c as f32) * step;
                let y = rect.top() + (r as f32) * step;
                let col = if hash % 2 == 0 {
                    egui::Color32::from_rgba_unmultiplied(255, 250, 240, alpha)
                } else {
                    egui::Color32::from_rgba_unmultiplied(20, 15, 10, alpha)
                };
                painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(1.5, 1.5)),
                    0.0,
                    col,
                );
            }
        }
    }
}

pub fn draw_dashed_line(
    painter: &egui::Painter,
    start: egui::Pos2,
    end: egui::Pos2,
    dash_len: f32,
    gap_len: f32,
    stroke: egui::Stroke,
) {
    let dir = end - start;
    let total_len = dir.length();
    if total_len < 1e-3 {
        return;
    }
    let unit = dir / total_len;
    let mut dist = 0.0;
    while dist < total_len {
        let seg_end = (dist + dash_len).min(total_len);
        painter.line_segment([start + unit * dist, start + unit * seg_end], stroke);
        dist += dash_len + gap_len;
    }
}

pub fn draw_composition_overlay(
    painter: &egui::Painter,
    rect: egui::Rect,
    overlay: CompositionOverlay,
) {
    let stroke = egui::Stroke::new(
        1.0,
        egui::Color32::from_rgba_unmultiplied(120, 220, 255, 110),
    );
    match overlay {
        CompositionOverlay::None => {}
        CompositionOverlay::RuleOfThirds => {
            for i in 1..=2 {
                let f = i as f32 / 3.0;
                let x = rect.left() + rect.width() * f;
                let y = rect.top() + rect.height() * f;
                painter.line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    stroke,
                );
                painter.line_segment(
                    [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                    stroke,
                );
            }
        }
        CompositionOverlay::GoldenRatio => {
            for &f in &[0.382, 0.618] {
                let x = rect.left() + rect.width() * f;
                let y = rect.top() + rect.height() * f;
                painter.line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    stroke,
                );
                painter.line_segment(
                    [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                    stroke,
                );
            }
        }
        CompositionOverlay::DiagonalCross => {
            painter.line_segment([rect.left_top(), rect.right_bottom()], stroke);
            painter.line_segment([rect.right_top(), rect.left_bottom()], stroke);
            painter.line_segment(
                [
                    egui::pos2(rect.center().x, rect.top()),
                    egui::pos2(rect.center().x, rect.bottom()),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(rect.left(), rect.center().y),
                    egui::pos2(rect.right(), rect.center().y),
                ],
                stroke,
            );
        }
        CompositionOverlay::PrintSafeMargin => {
            let safe_rect = rect.shrink(rect.width() * 0.06);
            painter.rect_stroke(safe_rect, 2.0, stroke, egui::StrokeKind::Middle);
        }
    }
}

pub fn draw_shape_on_painter(
    painter: &egui::Painter,
    shape: &CoverShape,
    centre: egui::Pos2,
    bounds_rect: egui::Rect,
    zoom: f32,
    anti_aliasing: bool,
) {
    if !shape.visible {
        return;
    }

    let repeats = shape.repeat_count.max(1);
    for r_idx in 0..repeats {
        let x = centre.x + (shape.x_offset + (r_idx as f32) * shape.repeat_step_x) * zoom;
        let y = centre.y + (shape.y_offset + (r_idx as f32) * shape.repeat_step_y) * zoom;

        match shape.shape_type {
            ShapeType::HorizontalLine { thickness } => {
                painter.line_segment(
                    [
                        egui::pos2(bounds_rect.left(), y),
                        egui::pos2(bounds_rect.right(), y),
                    ],
                    egui::Stroke::new((thickness * zoom).max(0.5), shape.colour),
                );
            }
            ShapeType::VerticalLine { thickness } => {
                painter.line_segment(
                    [
                        egui::pos2(x, bounds_rect.top()),
                        egui::pos2(x, bounds_rect.bottom()),
                    ],
                    egui::Stroke::new((thickness * zoom).max(0.5), shape.colour),
                );
            }
            _ => {
                for contour in shape.rotated_contours() {
                    let pts: Vec<egui::Pos2> = contour
                        .into_iter()
                        .map(|v| egui::pos2(x + v.x * zoom, y + v.y * zoom))
                        .collect();
                    if shape.filled {
                        draw_arbitrary_polygon(painter, &pts, shape.colour, anti_aliasing);
                    }
                    if shape.outlined && pts.len() >= 2 {
                        painter.add(egui::Shape::closed_line(
                            pts,
                            egui::Stroke::new(
                                (shape.stroke_width * zoom).max(0.5),
                                shape.stroke_colour,
                            ),
                        ));
                    }
                }
            }
        }
    }
}

pub fn draw_canvas_text_block(
    painter: &egui::Painter,
    tb: &TextBlock,
    cover_rect: egui::Rect,
    zoom: f32,
    font_family: &egui::FontFamily,
) {
    if !tb.visible || tb.content.is_empty() {
        return;
    }

    let formatted = tb.case_transform.apply(&tb.content);
    let lines: Vec<&str> = formatted.lines().collect();
    if lines.is_empty() {
        return;
    }

    let anchor_x = cover_rect.center().x + tb.x_offset * zoom;
    let anchor_y = cover_rect.top() + tb.y_pos * zoom;
    let scaled_font_size = (tb.font_size * zoom).max(1.0);
    let scaled_line_spacing = tb.line_spacing * zoom;
    let scaled_tracking = tb.letter_spacing * zoom;
    let rot_rad = tb.rotation_deg.to_radians();

    let font_id = egui::FontId::new(scaled_font_size, font_family.clone());
    let total_h = (lines.len().saturating_sub(1) as f32) * scaled_line_spacing;

    if tb.banner_box {
        let mut max_line_w: f32 = 20.0;
        for line in &lines {
            let g = painter.layout_no_wrap(line.to_string(), font_id.clone(), tb.colour);
            let extra = (line.chars().count().saturating_sub(1) as f32) * scaled_tracking;
            max_line_w = max_line_w.max(g.size().x + extra);
        }
        let bw = max_line_w + tb.banner_padding.x * 2.0 * zoom;
        let bh = total_h + scaled_font_size + tb.banner_padding.y * 2.0 * zoom;
        let banner_rect =
            egui::Rect::from_center_size(egui::pos2(anchor_x, anchor_y), egui::vec2(bw, bh));
        painter.rect_filled(banner_rect, tb.banner_rounding * zoom, tb.banner_colour);
    }

    for (line_idx, line) in lines.iter().enumerate() {
        let local_line_y = -total_h * 0.5 + (line_idx as f32) * scaled_line_spacing;

        if tb.arch_bend.abs() > 1.0 && line.len() > 1 {
            let chars: Vec<char> = line.chars().collect();
            let n = chars.len();
            let arc_rad = tb.arch_bend.to_radians();
            let approx_w = (n as f32) * (scaled_font_size * 0.55 + scaled_tracking).max(2.0);
            let radius = (approx_w / arc_rad.abs()).max(20.0);

            for (ci, ch) in chars.into_iter().enumerate() {
                let frac = if n > 1 {
                    (ci as f32 / (n - 1) as f32) - 0.5
                } else {
                    0.0
                };
                let theta = frac * arc_rad;
                let (char_local, char_rot) = if tb.arch_bend > 0.0 {
                    (
                        egui::vec2(
                            radius * theta.sin(),
                            local_line_y + radius * (1.0 - theta.cos()),
                        ),
                        rot_rad + theta,
                    )
                } else {
                    (
                        egui::vec2(
                            radius * theta.sin(),
                            local_line_y - radius * (1.0 - theta.cos()),
                        ),
                        rot_rad - theta,
                    )
                };

                let world_pos = egui::pos2(anchor_x, anchor_y) + rotate_vec(char_local, rot_rad);
                draw_styled_glyph_or_line(
                    painter,
                    &ch.to_string(),
                    world_pos,
                    char_rot,
                    &font_id,
                    tb,
                    zoom,
                    TextAlign::Centre,
                );
            }
        } else if tb.letter_spacing.abs() > 0.05 {
            let chars: Vec<char> = line.chars().collect();
            let mut glyphs = Vec::with_capacity(chars.len());
            let mut total_w = 0.0;
            for (ci, ch) in chars.iter().enumerate() {
                let g = painter.layout_no_wrap(ch.to_string(), font_id.clone(), tb.colour);
                let w = g.size().x;
                glyphs.push((ch.to_string(), w));
                total_w += w;
                if ci + 1 < chars.len() {
                    total_w += scaled_tracking;
                }
            }

            let start_x = match tb.alignment {
                TextAlign::Left => 0.0,
                TextAlign::Centre => -total_w * 0.5,
                TextAlign::Right => -total_w,
            };

            let mut cursor_x = start_x;
            for (ch_str, w) in glyphs {
                let char_local = egui::vec2(cursor_x + w * 0.5, local_line_y);
                let world_pos = egui::pos2(anchor_x, anchor_y) + rotate_vec(char_local, rot_rad);
                draw_styled_glyph_or_line(
                    painter,
                    &ch_str,
                    world_pos,
                    rot_rad,
                    &font_id,
                    tb,
                    zoom,
                    TextAlign::Centre,
                );
                cursor_x += w + scaled_tracking;
            }
        } else {
            let world_pos =
                egui::pos2(anchor_x, anchor_y) + rotate_vec(egui::vec2(0.0, local_line_y), rot_rad);
            draw_styled_glyph_or_line(
                painter,
                line,
                world_pos,
                rot_rad,
                &font_id,
                tb,
                zoom,
                tb.alignment,
            );
        }
    }
}

fn draw_styled_glyph_or_line(
    painter: &egui::Painter,
    text: &str,
    pos: egui::Pos2,
    angle_rad: f32,
    font_id: &egui::FontId,
    tb: &TextBlock,
    zoom: f32,
    align: TextAlign,
) {
    let paint_pass = |p: egui::Pos2, col: egui::Color32| {
        let galley = painter.layout_no_wrap(text.to_string(), font_id.clone(), col);
        let size = galley.size();
        let local_offset = match align {
            TextAlign::Left => egui::vec2(0.0, -size.y * 0.5),
            TextAlign::Centre => egui::vec2(-size.x * 0.5, -size.y * 0.5),
            TextAlign::Right => egui::vec2(-size.x, -size.y * 0.5),
        };
        let rotated_offset = rotate_vec(local_offset, angle_rad);
        painter.add(
            egui::epaint::TextShape::new(p + rotated_offset, galley, col).with_angle(angle_rad),
        );
    };

    if tb.shadow {
        paint_pass(pos + tb.shadow_offset * zoom, tb.shadow_colour);
    }

    if tb.outlined && tb.outline_width > 0.1 {
        let ow = tb.outline_width * zoom;
        for i in 0..8 {
            let a = (i as f32 / 8.0) * std::f32::consts::TAU;
            paint_pass(
                pos + egui::vec2(ow * a.cos(), ow * a.sin()),
                tb.outline_colour,
            );
        }
    }

    paint_pass(pos, tb.colour);

    if tb.faux_bold {
        let bw = (0.7 * zoom).max(0.5);
        paint_pass(pos + egui::vec2(bw, 0.0), tb.colour);
    }
}

/// Renders the entire interactive central canvas (Zoom, Pan, Dragging, Ctrl-Margin Snapping, Shapes & Text)
#[allow(clippy::too_many_arguments)]
pub fn render_interactive_canvas(
    ui: &mut egui::Ui,
    cover: &mut BookCover,
    installed_fonts: &[InstalledFont],
    canvas_zoom: &mut f32,
    canvas_pan: &mut egui::Vec2,
    selected_shape: &mut Option<usize>,
    selected_text: &mut Option<usize>,
    dragging_target: &mut Option<DragTarget>,
    drag_raw_offset: &mut egui::Vec2,
    pre_drag_snapshot: &mut Option<BookCover>,
    last_cover_screen_rect: &mut Option<egui::Rect>,
    enable_snapping: bool,
    show_all_outer_guides: bool,
    anti_aliasing: bool,
    composition_overlay: CompositionOverlay,
) -> Option<BookCover> {
    let mut completed_undo_snapshot: Option<BookCover> = None;

    let viewport_rect = ui.available_rect_before_wrap();
    let (_, response) = ui.allocate_exact_size(viewport_rect.size(), egui::Sense::click_and_drag());

    // --- 1. ZOOM & PAN INPUT HANDLING ---
    if response.hovered() {
        let scroll_y = ui.input(|i| i.smooth_scroll_delta.y);
        let zoom_delta = ui.input(|i| i.zoom_delta());

        let factor = if (zoom_delta - 1.0).abs() > 1e-3 {
            zoom_delta
        } else if scroll_y.abs() > 1e-3 {
            (scroll_y * 0.0025).exp()
        } else {
            1.0
        };

        if (factor - 1.0).abs() > 1e-4 {
            let old_zoom = *canvas_zoom;
            let new_zoom = (old_zoom * factor).clamp(0.25, 5.0);
            if let Some(mouse_pos) = response.hover_pos() {
                let rel = mouse_pos - (viewport_rect.center() + *canvas_pan);
                *canvas_pan -= rel * (new_zoom / old_zoom - 1.0);
            }
            *canvas_zoom = new_zoom;
        }
    }

    let space_held =
        !ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.key_down(egui::Key::Space));
    let is_panning = response.dragged_by(egui::PointerButton::Middle)
        || response.dragged_by(egui::PointerButton::Secondary)
        || (space_held && response.dragged_by(egui::PointerButton::Primary));

    if is_panning {
        *canvas_pan += response.drag_delta();
    }

    let zoom = *canvas_zoom;
    let scaled_cover_size = egui::vec2(cover.cover_width * zoom, cover.cover_height * zoom);
    let cover_centre = viewport_rect.center() + *canvas_pan;
    let cover_rect = egui::Rect::from_center_size(cover_centre, scaled_cover_size);
    *last_cover_screen_rect = Some(cover_rect);

    let inner_left =
        cover_rect.center().x + (cover.frame_x_offset - cover.frame_width * 0.5) * zoom;
    let inner_top = cover_rect.top() + cover.frame_y_offset * zoom;
    let inner_rect = egui::Rect::from_min_size(
        egui::pos2(inner_left, inner_top),
        egui::vec2(cover.frame_width * zoom, cover.frame_height * zoom),
    );
    let inner_centre = inner_rect.center();

    // --- 2. HIT TESTING, SELECTION & DRAGGING WITH CTRL OUTER-MARGIN SNAPPING ---
    let mut active_snap_x: Vec<f32> = Vec::new();
    let mut active_snap_y: Vec<f32> = Vec::new();
    let ctrl_held = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);

    if !is_panning && (response.drag_started_by(egui::PointerButton::Primary) || response.clicked())
    {
        if let Some(mouse_pos) = response.interact_pointer_pos() {
            let mut hit_target: Option<DragTarget> = None;

            // Check Text Blocks first (since they render on top)
            for (t_idx, tb) in cover.text_blocks.iter().enumerate().rev() {
                if !tb.visible || tb.locked {
                    continue;
                }
                let tx = cover_rect.center().x + tb.x_offset * zoom;
                let ty = cover_rect.top() + tb.y_pos * zoom;
                let line_cnt = tb.content.lines().count().max(1) as f32;
                let approx_w = (tb
                    .content
                    .lines()
                    .map(|l| l.len())
                    .max()
                    .unwrap_or(4) as f32)
                    * (tb.font_size * 0.55 + tb.letter_spacing).max(8.0)
                    * zoom;
                let approx_h = (line_cnt * tb.line_spacing * zoom).max(tb.font_size * zoom);
                let hit_rect = egui::Rect::from_center_size(
                    egui::pos2(tx, ty),
                    egui::vec2(approx_w.max(40.0), approx_h.max(24.0)),
                );
                if hit_rect.contains(mouse_pos) {
                    hit_target = Some(DragTarget::Text(t_idx));
                    break;
                }
            }

            // Check Shapes next (top-most first)
            if hit_target.is_none() {
                let unscaled_mouse = egui::pos2(
                    (mouse_pos.x - inner_centre.x) / zoom,
                    (mouse_pos.y - inner_centre.y) / zoom,
                );
                for (s_idx, shape) in cover.shapes.iter().enumerate().rev() {
                    if !shape.visible || shape.locked {
                        continue;
                    }
                    let repeats = shape.repeat_count.max(1);
                    let contours = shape.rotated_contours();

                    let hit = (0..repeats).any(|r_idx| {
                        let sx = shape.x_offset + (r_idx as f32) * shape.repeat_step_x;
                        let sy = shape.y_offset + (r_idx as f32) * shape.repeat_step_y;

                        match shape.shape_type {
                            ShapeType::HorizontalLine { thickness } => {
                                (unscaled_mouse.y - sy).abs() <= (thickness * 0.5).max(8.0)
                            }
                            ShapeType::VerticalLine { thickness } => {
                                (unscaled_mouse.x - sx).abs() <= (thickness * 0.5).max(8.0)
                            }
                            _ => contours.iter().any(|c| {
                                let poly: Vec<egui::Pos2> =
                                    c.iter().map(|v| egui::pos2(sx + v.x, sy + v.y)).collect();
                                point_in_polygon(unscaled_mouse, &poly)
                            }),
                        }
                    });

                    if hit {
                        hit_target = Some(DragTarget::Shape(s_idx));
                        break;
                    }
                }
            }

            match hit_target {
                Some(DragTarget::Shape(s_idx)) => {
                    *selected_shape = Some(s_idx);
                    *selected_text = None;
                    if response.drag_started_by(egui::PointerButton::Primary) {
                        *pre_drag_snapshot = Some(cover.clone());
                        *dragging_target = Some(DragTarget::Shape(s_idx));
                        *drag_raw_offset =
                            egui::vec2(cover.shapes[s_idx].x_offset, cover.shapes[s_idx].y_offset);
                    }
                }
                Some(DragTarget::Text(t_idx)) => {
                    *selected_text = Some(t_idx);
                    *selected_shape = None;
                    if response.drag_started_by(egui::PointerButton::Primary) {
                        *pre_drag_snapshot = Some(cover.clone());
                        *dragging_target = Some(DragTarget::Text(t_idx));
                        *drag_raw_offset = egui::vec2(
                            cover.text_blocks[t_idx].x_offset,
                            cover.text_blocks[t_idx].y_pos,
                        );
                    }
                }
                None => {
                    if response.clicked() {
                        *selected_shape = None;
                        *selected_text = None;
                    }
                }
            }
        }
    }

    if !is_panning && response.dragged_by(egui::PointerButton::Primary) {
        let delta = response.drag_delta() / zoom;
        match *dragging_target {
            Some(DragTarget::Shape(index)) => {
                *drag_raw_offset += delta;
                let mut new_x_offset = drag_raw_offset.x;
                let mut new_y_offset = drag_raw_offset.y;

                if enable_snapping {
                    let snap_threshold = 5.0 / zoom.sqrt();
                    let half_win_w = cover.frame_width * 0.5;
                    let half_win_h = cover.frame_height * 0.5;
                    let mut target_xs = vec![-half_win_w, 0.0, half_win_w];
                    let mut target_ys = vec![-half_win_h, 0.0, half_win_h];

                    for (other_i, other) in cover.shapes.iter().enumerate() {
                        if other_i == index || !other.visible {
                            continue;
                        }
                        let (other_xs, other_ys) = other.all_target_snap_anchors();
                        for ox in other_xs {
                            target_xs.push(other.x_offset + ox);
                        }
                        for oy in other_ys {
                            target_ys.push(other.y_offset + oy);
                        }
                    }

                    if let Some(dragged) = cover.shapes.get(index) {
                        // CTRL HELD -> Snap using the shape's Secondary Outer Margins!
                        // NO CTRL -> Snap using the shape's Regular Primary Anchors!
                        let (my_x_anchors, my_y_anchors) = if ctrl_held {
                            dragged.outer_margin_snap_anchors()
                        } else {
                            dragged.primary_snap_anchors()
                        };

                        let mut best_dx: Option<(f32, f32)> = None;
                        for anchor in &my_x_anchors {
                            let current_edge_x = drag_raw_offset.x + anchor;
                            for &target_x in &target_xs {
                                let diff = target_x - current_edge_x;
                                if diff.abs() <= snap_threshold
                                    && best_dx.map_or(true, |(best, _)| diff.abs() < best.abs())
                                {
                                    best_dx = Some((diff, target_x));
                                }
                            }
                        }

                        if let Some((diff, _)) = best_dx {
                            new_x_offset += diff;
                            for anchor in &my_x_anchors {
                                let snapped_edge = new_x_offset + anchor;
                                for &target_x in &target_xs {
                                    if (snapped_edge - target_x).abs() < 0.1 {
                                        active_snap_x.push(inner_centre.x + target_x * zoom);
                                    }
                                }
                            }
                        }

                        let mut best_dy: Option<(f32, f32)> = None;
                        for anchor in &my_y_anchors {
                            let current_edge_y = drag_raw_offset.y + anchor;
                            for &target_y in &target_ys {
                                let diff = target_y - current_edge_y;
                                if diff.abs() <= snap_threshold
                                    && best_dy.map_or(true, |(best, _)| diff.abs() < best.abs())
                                {
                                    best_dy = Some((diff, target_y));
                                }
                            }
                        }

                        if let Some((diff, _)) = best_dy {
                            new_y_offset += diff;
                            for anchor in &my_y_anchors {
                                let snapped_edge = new_y_offset + anchor;
                                for &target_y in &target_ys {
                                    if (snapped_edge - target_y).abs() < 0.1 {
                                        active_snap_y.push(inner_centre.y + target_y * zoom);
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(shape) = cover.shapes.get_mut(index) {
                    shape.x_offset = new_x_offset;
                    shape.y_offset = new_y_offset;
                }
            }
            Some(DragTarget::Text(t_idx)) => {
                *drag_raw_offset += delta;
                let mut new_x = drag_raw_offset.x;
                let new_y = drag_raw_offset.y;

                if enable_snapping && new_x.abs() <= 5.0 {
                    new_x = 0.0;
                    active_snap_x.push(cover_rect.center().x);
                }

                if let Some(tb) = cover.text_blocks.get_mut(t_idx) {
                    tb.x_offset = new_x;
                    tb.y_pos = new_y;
                }
            }
            None => {}
        }
    }

    if response.drag_stopped() {
        if dragging_target.is_some() {
            completed_undo_snapshot = pre_drag_snapshot.take();
        }
        *dragging_target = None;
    }

    // --- 3. RENDER COVER, INNER WINDOW, SHAPES & TEXT ---
    let canvas_painter = ui.painter_at(viewport_rect);
    let cover_painter = canvas_painter.with_clip_rect(viewport_rect.intersect(cover_rect));
    let inner_painter = canvas_painter.with_clip_rect(viewport_rect.intersect(inner_rect));

    // Outer Cover Background
    if cover.bg_gradient {
        draw_vertical_gradient_rect(
            &cover_painter,
            cover_rect,
            cover.bg_colour,
            cover.bg_colour_bottom,
        );
    } else {
        cover_painter.rect_filled(cover_rect, 4.0 * zoom, cover.bg_colour);
    }

    // Inner Window Background
    if cover.inner_gradient {
        draw_vertical_gradient_rect(
            &inner_painter,
            inner_rect,
            cover.inner_colour,
            cover.inner_colour_bottom,
        );
    } else {
        cover_painter.rect_filled(
            inner_rect,
            cover.frame_rounding * zoom,
            cover.inner_colour,
        );
    }

    // Clipped Shapes
    for shape in cover.shapes.iter().filter(|s| s.clip_to_window) {
        draw_shape_on_painter(
            &inner_painter,
            shape,
            inner_centre,
            inner_rect,
            zoom,
            anti_aliasing,
        );
    }

    // Inner Window Border Stroke
    if cover.frame_outlined {
        cover_painter.rect_stroke(
            inner_rect,
            cover.frame_rounding * zoom,
            egui::Stroke::new(
                (cover.frame_stroke_width * zoom).max(0.5),
                cover.frame_stroke_colour,
            ),
            egui::StrokeKind::Middle,
        );
    }

    // Unclipped Shapes (break outside the inner window onto the cover)
    for shape in cover.shapes.iter().filter(|s| !s.clip_to_window) {
        draw_shape_on_painter(
            &cover_painter,
            shape,
            inner_centre,
            cover_rect,
            zoom,
            anti_aliasing,
        );
    }

    // Text Blocks
    for tb in &cover.text_blocks {
        let font_family = installed_fonts
            .get(tb.font_index)
            .map(|f| &f.egui_family)
            .unwrap_or(&installed_fonts[0].egui_family);
        let target_painter = if tb.clip_to_window {
            &inner_painter
        } else {
            &cover_painter
        };
        draw_canvas_text_block(target_painter, tb, cover_rect, zoom, font_family);
    }

    // Vintage Paper Grain Overlay
    draw_paper_grain_overlay(&cover_painter, cover_rect, cover.paper_grain, zoom);

    // Composition Grid Overlay
    draw_composition_overlay(&cover_painter, cover_rect, composition_overlay);

    // Selection Highlight Box when not actively dragging
    if dragging_target.is_none() {
        if let Some(sel_idx) = *selected_shape {
            if let Some(shape) = cover.shapes.get(sel_idx) {
                if shape.visible {
                    let (min_x, max_x, min_y, max_y) = shape.local_bounds();
                    let sx = inner_centre.x + shape.x_offset * zoom;
                    let sy = inner_centre.y + shape.y_offset * zoom;
                    let sel_rect = egui::Rect::from_min_max(
                        egui::pos2(sx + min_x * zoom - 3.0, sy + min_y * zoom - 3.0),
                        egui::pos2(sx + max_x * zoom + 3.0, sy + max_y * zoom + 3.0),
                    );
                    cover_painter.rect_stroke(
                        sel_rect,
                        2.0,
                        egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgba_unmultiplied(80, 200, 255, 160),
                        ),
                        egui::StrokeKind::Outside,
                    );
                }
            }
        }
    }

    // --- 4. DRAW SNAPPING GUIDES, PRIMARY BOUNDS & CTRL OUTER ALIGNMENT MARGINS ---
    if let Some(DragTarget::Shape(index)) = *dragging_target {
        if show_all_outer_guides {
            let other_outer_stroke = egui::Stroke::new(
                1.0,
                egui::Color32::from_rgba_unmultiplied(255, 165, 0, 125),
            );
            for (other_i, other) in cover.shapes.iter().enumerate() {
                if other_i == index || !other.enable_outer_align || !other.visible {
                    continue;
                }
                let (min_x, max_x, min_y, max_y) = other.local_bounds();
                let ox = inner_centre.x + other.x_offset * zoom;
                let oy = inner_centre.y + other.y_offset * zoom;
                let off = other.align_offset * zoom;

                if (max_x - min_x) > 0.1
                    || matches!(other.shape_type, ShapeType::VerticalLine { .. })
                {
                    for lx in [ox + min_x * zoom - off, ox + max_x * zoom + off] {
                        draw_dashed_line(
                            &inner_painter,
                            egui::pos2(lx, inner_rect.top()),
                            egui::pos2(lx, inner_rect.bottom()),
                            5.0,
                            4.0,
                            other_outer_stroke,
                        );
                    }
                }
                if (max_y - min_y) > 0.1
                    || matches!(other.shape_type, ShapeType::HorizontalLine { .. })
                {
                    for ly in [oy + min_y * zoom - off, oy + max_y * zoom + off] {
                        draw_dashed_line(
                            &inner_painter,
                            egui::pos2(inner_rect.left(), ly),
                            egui::pos2(inner_rect.right(), ly),
                            5.0,
                            4.0,
                            other_outer_stroke,
                        );
                    }
                }
            }
        }

        if let Some(dragged) = cover.shapes.get(index) {
            let (min_x, max_x, min_y, max_y) = dragged.local_bounds();
            let sx = inner_centre.x + dragged.x_offset * zoom;
            let sy = inner_centre.y + dragged.y_offset * zoom;

            // Highlight whichever anchor set is currently active (Ctrl = Outer Margin, Normal = Primary)
            let primary_alpha = if ctrl_held { 65 } else { 150 };
            let outer_alpha = if ctrl_held { 240 } else { 180 };

            let primary_guide_stroke = egui::Stroke::new(
                1.0,
                egui::Color32::from_rgba_unmultiplied(0, 180, 255, primary_alpha),
            );
            let outer_guide_stroke = egui::Stroke::new(
                if ctrl_held { 1.8 } else { 1.2 },
                egui::Color32::from_rgba_unmultiplied(255, 190, 30, outer_alpha),
            );

            let x_lines = if (max_x - min_x) > 0.1 {
                vec![sx + min_x * zoom, sx, sx + max_x * zoom]
            } else {
                vec![sx]
            };
            for lx in x_lines {
                inner_painter.line_segment(
                    [
                        egui::pos2(lx, inner_rect.top()),
                        egui::pos2(lx, inner_rect.bottom()),
                    ],
                    primary_guide_stroke,
                );
            }

            let y_lines = if (max_y - min_y) > 0.1 {
                vec![sy + min_y * zoom, sy, sy + max_y * zoom]
            } else {
                vec![sy]
            };
            for ly in y_lines {
                inner_painter.line_segment(
                    [
                        egui::pos2(inner_rect.left(), ly),
                        egui::pos2(inner_rect.right(), ly),
                    ],
                    primary_guide_stroke,
                );
            }

            // Show Outer Alignment Margin Lines if enabled on the shape OR if Ctrl is held!
            if (dragged.enable_outer_align || ctrl_held) && dragged.align_offset.abs() > 0.1 {
                let off = dragged.align_offset * zoom;
                if (max_x - min_x) > 0.1
                    || matches!(dragged.shape_type, ShapeType::VerticalLine { .. })
                {
                    for lx in [sx + min_x * zoom - off, sx + max_x * zoom + off] {
                        draw_dashed_line(
                            &inner_painter,
                            egui::pos2(lx, inner_rect.top()),
                            egui::pos2(lx, inner_rect.bottom()),
                            6.0,
                            4.0,
                            outer_guide_stroke,
                        );
                    }
                }
                if (max_y - min_y) > 0.1
                    || matches!(dragged.shape_type, ShapeType::HorizontalLine { .. })
                {
                    for ly in [sy + min_y * zoom - off, sy + max_y * zoom + off] {
                        draw_dashed_line(
                            &inner_painter,
                            egui::pos2(inner_rect.left(), ly),
                            egui::pos2(inner_rect.right(), ly),
                            6.0,
                            4.0,
                            outer_guide_stroke,
                        );
                    }
                }
            }
        }
    }

    // Active Snap Magenta Lines
    let snap_stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 50, 180));
    for sx in active_snap_x {
        canvas_painter.line_segment(
            [
                egui::pos2(sx, inner_rect.top() - 10.0),
                egui::pos2(sx, inner_rect.bottom() + 10.0),
            ],
            snap_stroke,
        );
    }
    for sy in active_snap_y {
        canvas_painter.line_segment(
            [
                egui::pos2(inner_rect.left() - 10.0, sy),
                egui::pos2(inner_rect.right() + 10.0, sy),
            ],
            snap_stroke,
        );
    }

    // --- 5. FLOATING TOP-LEFT ZOOM & PAN HUD ---
    let hud_rect = egui::Rect::from_min_size(
        viewport_rect.min + egui::vec2(12.0, 12.0),
        egui::vec2(340.0, 34.0),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(hud_rect), |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("🔍 {:3.0}%", *canvas_zoom * 100.0));
                ui.add(
                    egui::Slider::new(canvas_zoom, 0.25..=5.0)
                        .show_value(false)
                        .text(""),
                );
                if ui.small_button("100%").clicked() {
                    *canvas_zoom = 1.0;
                }
                if ui
                    .small_button("🎯 Centre View")
                    .on_hover_text("Reset zoom and pan to centre")
                    .clicked()
                {
                    *canvas_zoom = 1.0;
                    *canvas_pan = egui::Vec2::ZERO;
                }
                if ctrl_held {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 195, 40),
                        "⚡ Ctrl: Outer Margin Snap",
                    );
                }
            });
        });
    });

    completed_undo_snapshot
}