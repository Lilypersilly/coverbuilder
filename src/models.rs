use eframe::egui;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct InstalledFont {
    pub display_name: String,
    pub egui_family: egui::FontFamily,
    pub svg_family: String,
    pub file_path: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct ColourPalette {
    pub name: String,
    pub bg_colour: egui::Color32,
    pub inner_colour: egui::Color32,
    pub text_colour: egui::Color32,
    pub shape_colour: egui::Color32,
    pub built_in: bool,
}

pub fn default_palettes() -> Vec<ColourPalette> {
    vec![
        ColourPalette {
            name: "🍡 Marshmellow".to_string(),
            bg_colour: egui::Color32::from_rgb(255, 178, 221),
            inner_colour: egui::Color32::from_rgb(252, 232, 250),
            text_colour: egui::Color32::WHITE,
            shape_colour: egui::Color32::WHITE,
            built_in: true,
        },
        ColourPalette {
            name: "🌙 Midnight Gold".to_string(),
            bg_colour: egui::Color32::from_rgb(20, 28, 48),
            inner_colour: egui::Color32::from_rgb(32, 44, 72),
            text_colour: egui::Color32::from_rgb(238, 198, 108),
            shape_colour: egui::Color32::from_rgb(238, 198, 108),
            built_in: true,
        },
        ColourPalette {
            name: "🍵 Matcha Latte".to_string(),
            bg_colour: egui::Color32::from_rgb(138, 168, 130),
            inner_colour: egui::Color32::from_rgb(242, 238, 225),
            text_colour: egui::Color32::from_rgb(250, 248, 242),
            shape_colour: egui::Color32::from_rgb(92, 122, 86),
            built_in: true,
        },
        ColourPalette {
            name: "🏺 Terracotta".to_string(),
            bg_colour: egui::Color32::from_rgb(198, 107, 77),
            inner_colour: egui::Color32::from_rgb(244, 224, 204),
            text_colour: egui::Color32::from_rgb(252, 240, 228),
            shape_colour: egui::Color32::from_rgb(198, 107, 77),
            built_in: true,
        },
        ColourPalette {
            name: "❄️ Nordic Frost".to_string(),
            bg_colour: egui::Color32::from_rgb(76, 106, 130),
            inner_colour: egui::Color32::from_rgb(224, 236, 244),
            text_colour: egui::Color32::WHITE,
            shape_colour: egui::Color32::from_rgb(76, 106, 130),
            built_in: true,
        },
        ColourPalette {
            name: "⚡ Cyberpunk".to_string(),
            bg_colour: egui::Color32::from_rgb(15, 12, 28),
            inner_colour: egui::Color32::from_rgb(34, 18, 56),
            text_colour: egui::Color32::from_rgb(0, 245, 212),
            shape_colour: egui::Color32::from_rgb(255, 42, 133),
            built_in: true,
        },
        ColourPalette {
            name: "🐧 Classic Penguin".to_string(),
            bg_colour: egui::Color32::from_rgb(242, 108, 34),
            inner_colour: egui::Color32::from_rgb(250, 246, 238),
            text_colour: egui::Color32::from_rgb(30, 30, 30),
            shape_colour: egui::Color32::from_rgb(30, 30, 30),
            built_in: true,
        },
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurveFunction {
    QuadraticArch,
    SineBell,
    SCurveWave,
    SharpVPeak,
    CircularArc,
}

impl CurveFunction {
    pub const ALL: [CurveFunction; 5] = [
        CurveFunction::QuadraticArch,
        CurveFunction::SineBell,
        CurveFunction::SCurveWave,
        CurveFunction::SharpVPeak,
        CurveFunction::CircularArc,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            CurveFunction::QuadraticArch => "Quadratic Arch (Parabolic)",
            CurveFunction::SineBell => "Sine / Bell Wave (Smooth Pinch)",
            CurveFunction::SCurveWave => "S-Curve (Inflection Wave)",
            CurveFunction::SharpVPeak => "Sharp V-Peak (Star / Tent)",
            CurveFunction::CircularArc => "Circular Dome Arc",
        }
    }

    pub fn to_id(self) -> &'static str {
        match self {
            CurveFunction::QuadraticArch => "Quad",
            CurveFunction::SineBell => "Sine",
            CurveFunction::SCurveWave => "SCurve",
            CurveFunction::SharpVPeak => "VPeak",
            CurveFunction::CircularArc => "Circ",
        }
    }

    pub fn from_id(s: &str) -> Self {
        match s {
            "Sine" => CurveFunction::SineBell,
            "SCurve" => CurveFunction::SCurveWave,
            "VPeak" => CurveFunction::SharpVPeak,
            "Circ" => CurveFunction::CircularArc,
            _ => CurveFunction::QuadraticArch,
        }
    }

    pub fn evaluate(self, start: egui::Vec2, end: egui::Vec2, curve: f32, t: f32) -> egui::Vec2 {
        let edge_vec = end - start;
        let len = edge_vec.length().max(1e-4);
        if curve.abs() < 1e-3 {
            return start + edge_vec * t;
        }
        let outward_normal = egui::vec2(-edge_vec.y / len, edge_vec.x / len);
        let linear = start + edge_vec * t;

        match self {
            CurveFunction::QuadraticArch => {
                let offset = 4.0 * t * (1.0 - t) * curve;
                linear + outward_normal * offset
            }
            CurveFunction::SineBell => {
                let s = (std::f32::consts::PI * t).sin();
                let offset = (s * s) * curve;
                linear + outward_normal * offset
            }
            CurveFunction::SCurveWave => {
                let offset = (std::f32::consts::TAU * t).sin() * curve;
                linear + outward_normal * offset
            }
            CurveFunction::SharpVPeak => {
                let factor = 1.0 - (2.0 * t - 1.0).abs();
                linear + outward_normal * (factor * curve)
            }
            CurveFunction::CircularArc => {
                let u = 2.0 * t - 1.0;
                let factor = (1.0 - u * u).max(0.0).sqrt();
                linear + outward_normal * (factor * curve)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddShapeKind {
    Ellipse,
    HalfEllipse,
    Rectangle,
    Triangle,
    PolygonStar,
    HorizontalLine,
    VerticalLine,
    ImportSvg,
}

impl AddShapeKind {
    pub const ALL: [AddShapeKind; 8] = [
        AddShapeKind::Ellipse,
        AddShapeKind::HalfEllipse,
        AddShapeKind::Rectangle,
        AddShapeKind::Triangle,
        AddShapeKind::PolygonStar,
        AddShapeKind::HorizontalLine,
        AddShapeKind::VerticalLine,
        AddShapeKind::ImportSvg,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            AddShapeKind::Ellipse => "⚪ Circle / Ellipse",
            AddShapeKind::HalfEllipse => "🌓 Half-Circle / Dome",
            AddShapeKind::Rectangle => "⬛ Rectangle / Curved Square",
            AddShapeKind::Triangle => "🔺 Triangle (Curvable)",
            AddShapeKind::PolygonStar => "⭐ Regular Polygon / Star",
            AddShapeKind::HorizontalLine => "➖ Horizontal Line",
            AddShapeKind::VerticalLine => "➗ Vertical Line",
            AddShapeKind::ImportSvg => "📥 Import External SVG Shape",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ShapeType {
    Ellipse {
        radius_x: f32,
        radius_y: f32,
        lock_aspect: bool,
    },
    HalfEllipse {
        radius_x: f32,
        radius_y: f32,
        lock_aspect: bool,
    },
    HorizontalLine {
        thickness: f32,
    },
    VerticalLine {
        thickness: f32,
    },
    Rectangle {
        width: f32,
        height: f32,
        curve_func: CurveFunction,
        link_curves: bool,
        curve_top: f32,
        curve_right: f32,
        curve_bottom: f32,
        curve_left: f32,
    },
    Triangle {
        side_a: f32,
        side_b: f32,
        side_c: f32,
        curve_func: CurveFunction,
        curve_a: f32,
        curve_b: f32,
        curve_c: f32,
    },
    PolygonStar {
        sides: usize,
        outer_radius: f32,
        inner_radius: f32,
        use_star: bool,
        curve_func: CurveFunction,
        edge_curve: f32,
    },
    ImportedSvg {
        source_path: String,
        scale_x: f32,
        scale_y: f32,
        lock_aspect: bool,
        contours: Vec<Vec<egui::Vec2>>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct CoverShape {
    pub name: String,
    pub shape_type: ShapeType,
    pub x_offset: f32,
    pub y_offset: f32,
    pub rotation_deg: f32,
    pub colour: egui::Color32,
    pub filled: bool,
    pub outlined: bool,
    pub stroke_width: f32,
    pub stroke_colour: egui::Color32,
    pub enable_outer_align: bool,
    pub align_offset: f32,
    pub visible: bool,
    pub locked: bool,
    pub clip_to_window: bool,
    pub repeat_count: usize,
    pub repeat_step_x: f32,
    pub repeat_step_y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextCase {
    AsTyped,
    Uppercase,
    Lowercase,
    TitleCase,
}

impl TextCase {
    pub const ALL: [TextCase; 4] = [
        TextCase::AsTyped,
        TextCase::Uppercase,
        TextCase::Lowercase,
        TextCase::TitleCase,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TextCase::AsTyped => "As Typed",
            TextCase::Uppercase => "UPPERCASE",
            TextCase::Lowercase => "lowercase",
            TextCase::TitleCase => "Title Case",
        }
    }

    pub fn to_id(self) -> &'static str {
        match self {
            TextCase::AsTyped => "AsTyped",
            TextCase::Uppercase => "Upper",
            TextCase::Lowercase => "Lower",
            TextCase::TitleCase => "Title",
        }
    }

    pub fn from_id(s: &str) -> Self {
        match s {
            "Upper" => TextCase::Uppercase,
            "Lower" => TextCase::Lowercase,
            "Title" => TextCase::TitleCase,
            _ => TextCase::AsTyped,
        }
    }

    pub fn apply(self, s: &str) -> String {
        match self {
            TextCase::AsTyped => s.to_string(),
            TextCase::Uppercase => s.to_uppercase(),
            TextCase::Lowercase => s.to_lowercase(),
            TextCase::TitleCase => s
                .lines()
                .map(|line| {
                    line.split_whitespace()
                        .map(|w| {
                            let mut c = w.chars();
                            match c.next() {
                                None => String::new(),
                                Some(f) => {
                                    f.to_uppercase().collect::<String>()
                                        + &c.as_str().to_lowercase()
                                }
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Centre,
    Right,
}

impl TextAlign {
    pub const ALL: [TextAlign; 3] = [TextAlign::Left, TextAlign::Centre, TextAlign::Right];

    pub fn label(self) -> &'static str {
        match self {
            TextAlign::Left => "Left",
            TextAlign::Centre => "Centre",
            TextAlign::Right => "Right",
        }
    }

    pub fn to_id(self) -> &'static str {
        match self {
            TextAlign::Left => "Left",
            TextAlign::Centre => "Centre",
            TextAlign::Right => "Right",
        }
    }

    pub fn from_id(s: &str) -> Self {
        match s {
            "Left" => TextAlign::Left,
            "Right" => TextAlign::Right,
            _ => TextAlign::Centre,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextBlock {
    pub name: String,
    pub content: String,
    pub font_index: usize,
    pub font_size: f32,
    pub line_spacing: f32,
    pub letter_spacing: f32,
    pub x_offset: f32,
    pub y_pos: f32,
    pub rotation_deg: f32,
    pub colour: egui::Color32,
    pub case_transform: TextCase,
    pub alignment: TextAlign,
    pub faux_bold: bool,
    pub outlined: bool,
    pub outline_colour: egui::Color32,
    pub outline_width: f32,
    pub shadow: bool,
    pub shadow_colour: egui::Color32,
    pub shadow_offset: egui::Vec2,
    pub banner_box: bool,
    pub banner_colour: egui::Color32,
    pub banner_padding: egui::Vec2,
    pub banner_rounding: f32,
    pub arch_bend: f32,
    pub clip_to_window: bool,
    pub visible: bool,
    pub locked: bool,
}

impl TextBlock {
    pub fn new_author(author: &str, colour: egui::Color32) -> Self {
        Self {
            name: "Author Name".to_string(),
            content: author.to_string(),
            font_index: 0,
            font_size: 34.0,
            line_spacing: 38.0,
            letter_spacing: 0.0,
            x_offset: 0.0,
            y_pos: 38.0,
            rotation_deg: 0.0,
            colour,
            case_transform: TextCase::AsTyped,
            alignment: TextAlign::Centre,
            faux_bold: false,
            outlined: false,
            outline_colour: egui::Color32::BLACK,
            outline_width: 1.5,
            shadow: false,
            shadow_colour: egui::Color32::from_black_alpha(120),
            shadow_offset: egui::vec2(2.0, 2.0),
            banner_box: false,
            banner_colour: egui::Color32::from_black_alpha(160),
            banner_padding: egui::vec2(14.0, 6.0),
            banner_rounding: 4.0,
            arch_bend: 0.0,
            clip_to_window: false,
            visible: true,
            locked: false,
        }
    }

    pub fn new_title(title: &str, colour: egui::Color32) -> Self {
        Self {
            name: "Book Title".to_string(),
            content: title.to_string(),
            font_index: 0,
            font_size: 36.0,
            line_spacing: 40.0,
            letter_spacing: 0.0,
            x_offset: 0.0,
            y_pos: 480.0,
            rotation_deg: 0.0,
            colour,
            case_transform: TextCase::AsTyped,
            alignment: TextAlign::Centre,
            faux_bold: false,
            outlined: false,
            outline_colour: egui::Color32::BLACK,
            outline_width: 1.5,
            shadow: false,
            shadow_colour: egui::Color32::from_black_alpha(120),
            shadow_offset: egui::vec2(2.0, 2.0),
            banner_box: false,
            banner_colour: egui::Color32::from_black_alpha(160),
            banner_padding: egui::vec2(16.0, 8.0),
            banner_rounding: 4.0,
            arch_bend: 0.0,
            clip_to_window: false,
            visible: true,
            locked: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositionOverlay {
    None,
    RuleOfThirds,
    GoldenRatio,
    DiagonalCross,
    PrintSafeMargin,
}

impl CompositionOverlay {
    pub const ALL: [CompositionOverlay; 5] = [
        CompositionOverlay::None,
        CompositionOverlay::RuleOfThirds,
        CompositionOverlay::GoldenRatio,
        CompositionOverlay::DiagonalCross,
        CompositionOverlay::PrintSafeMargin,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CompositionOverlay::None => "None",
            CompositionOverlay::RuleOfThirds => "Rule of Thirds Grid",
            CompositionOverlay::GoldenRatio => "Golden Ratio (Phi) Grid",
            CompositionOverlay::DiagonalCross => "Centre & Diagonal Cross",
            CompositionOverlay::PrintSafeMargin => "Print Bleed & Safe Margin",
        }
    }
}

pub fn rotate_vec(v: egui::Vec2, angle_rad: f32) -> egui::Vec2 {
    let (s, c) = angle_rad.sin_cos();
    egui::vec2(v.x * c - v.y * s, v.x * s + v.y * c)
}

pub fn triangle_local_vertices(side_a: f32, side_b: f32, side_c: f32) -> [egui::Vec2; 3] {
    let a = side_a.max(1.0);
    let b = side_b.max(1.0);
    let min_c = (a - b).abs() + 0.5;
    let max_c = a + b - 0.5;
    let c = side_c.clamp(min_c, max_c.max(min_c));

    let cos_angle = ((a * a + c * c - b * b) / (2.0 * a * c)).clamp(-1.0, 1.0);
    let sin_angle = (1.0 - cos_angle * cos_angle).max(0.0).sqrt();

    let p0 = egui::vec2(0.0, 0.0);
    let p1 = egui::vec2(a, 0.0);
    let p2 = egui::vec2(c * cos_angle, -c * sin_angle);

    let centroid = (p0 + p1 + p2) / 3.0;
    [p0 - centroid, p1 - centroid, p2 - centroid]
}

pub fn point_in_polygon(pt: egui::Pos2, poly: &[egui::Pos2]) -> bool {
    let mut inside = false;
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let pi = poly[i];
        let pj = poly[j];
        if ((pi.y > pt.y) != (pj.y > pt.y))
            && (pt.x < (pj.x - pi.x) * (pt.y - pi.y) / (pj.y - pi.y + 1e-6) + pi.x)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

pub fn sample_curved_edges(
    edges: &[(egui::Vec2, egui::Vec2, f32)],
    curve_func: CurveFunction,
    rot: f32,
) -> Vec<egui::Vec2> {
    let steps_per_edge = match curve_func {
        CurveFunction::SharpVPeak => 2,
        _ => 64,
    };
    let mut pts = Vec::with_capacity(edges.len() * steps_per_edge);
    for &(start, end, curve) in edges {
        let steps = if curve.abs() < 0.05 { 1 } else { steps_per_edge };
        for step in 0..steps {
            let t = step as f32 / steps as f32;
            let p = curve_func.evaluate(start, end, curve, t);
            pts.push(rotate_vec(p, rot));
        }
    }
    pts
}

impl CoverShape {
    pub fn new_with_defaults(name: String, shape_type: ShapeType, colour: egui::Color32) -> Self {
        Self {
            name,
            shape_type,
            x_offset: 0.0,
            y_offset: 0.0,
            rotation_deg: 0.0,
            colour,
            filled: true,
            outlined: false,
            stroke_width: 3.0,
            stroke_colour: colour,
            enable_outer_align: false,
            align_offset: 18.0,
            visible: true,
            locked: false,
            clip_to_window: true,
            repeat_count: 1,
            repeat_step_x: 25.0,
            repeat_step_y: 0.0,
        }
    }

    pub fn supports_rotation(&self) -> bool {
        !matches!(
            self.shape_type,
            ShapeType::HorizontalLine { .. } | ShapeType::VerticalLine { .. }
        )
    }

    pub fn rotated_contours(&self) -> Vec<Vec<egui::Vec2>> {
        let rot = self.rotation_deg.to_radians();
        match &self.shape_type {
            ShapeType::Ellipse {
                radius_x, radius_y, ..
            } => {
                let steps = 128;
                let pts = (0..steps)
                    .map(|i| {
                        let a = (i as f32 / steps as f32) * std::f32::consts::TAU;
                        let local = egui::vec2(*radius_x * a.cos(), *radius_y * a.sin());
                        rotate_vec(local, rot)
                    })
                    .collect();
                vec![pts]
            }
            ShapeType::HalfEllipse {
                radius_x, radius_y, ..
            } => {
                let steps = 96;
                let pts = (0..=steps)
                    .map(|i| {
                        let a = std::f32::consts::PI
                            + (i as f32 / steps as f32) * std::f32::consts::PI;
                        let local = egui::vec2(*radius_x * a.cos(), *radius_y * a.sin());
                        rotate_vec(local, rot)
                    })
                    .collect();
                vec![pts]
            }
            ShapeType::Rectangle {
                width,
                height,
                curve_func,
                curve_top,
                curve_right,
                curve_bottom,
                curve_left,
                ..
            } => {
                let hw = width / 2.0;
                let hh = height / 2.0;
                let tr = egui::vec2(hw, -hh);
                let tl = egui::vec2(-hw, -hh);
                let bl = egui::vec2(-hw, hh);
                let br = egui::vec2(hw, hh);

                let edges = [
                    (tr, tl, *curve_top),
                    (tl, bl, *curve_left),
                    (bl, br, *curve_bottom),
                    (br, tr, *curve_right),
                ];
                vec![sample_curved_edges(&edges, *curve_func, rot)]
            }
            ShapeType::Triangle {
                side_a,
                side_b,
                side_c,
                curve_func,
                curve_a,
                curve_b,
                curve_c,
            } => {
                let [v0, v1, v2] = triangle_local_vertices(*side_a, *side_b, *side_c);
                let edges = [(v0, v1, *curve_a), (v1, v2, *curve_b), (v2, v0, *curve_c)];
                vec![sample_curved_edges(&edges, *curve_func, rot)]
            }
            ShapeType::PolygonStar {
                sides,
                outer_radius,
                inner_radius,
                use_star,
                curve_func,
                edge_curve,
            } => {
                let n = (*sides).max(3);
                let mut verts = Vec::new();
                if *use_star {
                    let total = n * 2;
                    for i in 0..total {
                        let a = -std::f32::consts::FRAC_PI_2
                            + (i as f32 / total as f32) * std::f32::consts::TAU;
                        let r = if i % 2 == 0 {
                            *outer_radius
                        } else {
                            *inner_radius
                        };
                        verts.push(egui::vec2(r * a.cos(), r * a.sin()));
                    }
                } else {
                    for i in 0..n {
                        let a = -std::f32::consts::FRAC_PI_2
                            + (i as f32 / n as f32) * std::f32::consts::TAU;
                        verts.push(egui::vec2(*outer_radius * a.cos(), *outer_radius * a.sin()));
                    }
                }
                verts.reverse();
                let mut edges = Vec::with_capacity(verts.len());
                for i in 0..verts.len() {
                    edges.push((verts[i], verts[(i + 1) % verts.len()], *edge_curve));
                }
                vec![sample_curved_edges(&edges, *curve_func, rot)]
            }
            ShapeType::ImportedSvg {
                scale_x,
                scale_y,
                contours,
                ..
            } => contours
                .iter()
                .map(|c| {
                    c.iter()
                        .map(|p| rotate_vec(egui::vec2(p.x * *scale_x, p.y * *scale_y), rot))
                        .collect()
                })
                .collect(),
            ShapeType::HorizontalLine { .. } | ShapeType::VerticalLine { .. } => Vec::new(),
        }
    }

    pub fn local_bounds(&self) -> (f32, f32, f32, f32) {
        let (base_min_x, base_max_x, base_min_y, base_max_y) = match self.shape_type {
            ShapeType::HorizontalLine { thickness } => (0.0, 0.0, -thickness / 2.0, thickness / 2.0),
            ShapeType::VerticalLine { thickness } => (-thickness / 2.0, thickness / 2.0, 0.0, 0.0),
            _ => {
                let contours = self.rotated_contours();
                let mut min_x = f32::MAX;
                let mut max_x = f32::MIN;
                let mut min_y = f32::MAX;
                let mut max_y = f32::MIN;
                for c in contours {
                    for p in c {
                        min_x = min_x.min(p.x);
                        max_x = max_x.max(p.x);
                        min_y = min_y.min(p.y);
                        max_y = max_y.max(p.y);
                    }
                }
                if min_x > max_x {
                    (0.0, 0.0, 0.0, 0.0)
                } else {
                    (min_x, max_x, min_y, max_y)
                }
            }
        };

        let repeats = self.repeat_count.max(1);
        if repeats <= 1 {
            (base_min_x, base_max_x, base_min_y, base_max_y)
        } else {
            let last_dx = ((repeats - 1) as f32) * self.repeat_step_x;
            let last_dy = ((repeats - 1) as f32) * self.repeat_step_y;
            let min_x = base_min_x.min(base_min_x + last_dx);
            let max_x = base_max_x.max(base_max_x + last_dx);
            let min_y = base_min_y.min(base_min_y + last_dy);
            let max_y = base_max_y.max(base_max_y + last_dy);
            (min_x, max_x, min_y, max_y)
        }
    }

    pub fn primary_snap_anchors(&self) -> (Vec<f32>, Vec<f32>) {
        let (min_x, max_x, min_y, max_y) = self.local_bounds();
        let mut xs = vec![0.0];
        let mut ys = vec![0.0];
        if (max_x - min_x) > 0.1 {
            xs.push(min_x);
            xs.push(max_x);
        }
        if (max_y - min_y) > 0.1 {
            ys.push(min_y);
            ys.push(max_y);
        }
        (xs, ys)
    }

    pub fn outer_margin_snap_anchors(&self) -> (Vec<f32>, Vec<f32>) {
        let (min_x, max_x, min_y, max_y) = self.local_bounds();
        let off = self.align_offset;
        let mut xs = Vec::new();
        let mut ys = Vec::new();

        if (max_x - min_x) > 0.1 || matches!(self.shape_type, ShapeType::VerticalLine { .. }) {
            xs.push(min_x - off);
            xs.push(max_x + off);
        } else {
            xs.push(0.0);
        }

        if (max_y - min_y) > 0.1 || matches!(self.shape_type, ShapeType::HorizontalLine { .. }) {
            ys.push(min_y - off);
            ys.push(max_y + off);
        } else {
            ys.push(0.0);
        }
        (xs, ys)
    }

    pub fn all_target_snap_anchors(&self) -> (Vec<f32>, Vec<f32>) {
        let (mut xs, mut ys) = self.primary_snap_anchors();
        if self.enable_outer_align && self.align_offset.abs() > 0.1 {
            let (oxs, oys) = self.outer_margin_snap_anchors();
            xs.extend(oxs);
            ys.extend(oys);
        }
        (xs, ys)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BookCover {
    pub cover_width: f32,
    pub cover_height: f32,
    pub bg_colour: egui::Color32,
    pub bg_gradient: bool,
    pub bg_colour_bottom: egui::Color32,
    pub inner_colour: egui::Color32,
    pub inner_gradient: bool,
    pub inner_colour_bottom: egui::Color32,
    pub paper_grain: f32,
    pub default_shape_colour: egui::Color32,
    pub frame_width: f32,
    pub frame_height: f32,
    pub frame_x_offset: f32,
    pub frame_y_offset: f32,
    pub frame_rounding: f32,
    pub frame_outlined: bool,
    pub frame_stroke_width: f32,
    pub frame_stroke_colour: egui::Color32,
    pub shapes: Vec<CoverShape>,
    pub text_blocks: Vec<TextBlock>,
}

impl Default for BookCover {
    fn default() -> Self {
        let palettes = default_palettes();
        let marshmallow = &palettes[0];
        Self {
            cover_width: 380.0,
            cover_height: 560.0,
            bg_colour: marshmallow.bg_colour,
            bg_gradient: false,
            bg_colour_bottom: egui::Color32::from_rgb(225, 140, 190),
            inner_colour: marshmallow.inner_colour,
            inner_gradient: false,
            inner_colour_bottom: egui::Color32::WHITE,
            paper_grain: 0.0,
            default_shape_colour: marshmallow.shape_colour,
            frame_width: 324.0,
            frame_height: 324.0,
            frame_x_offset: 0.0,
            frame_y_offset: 75.0,
            frame_rounding: 4.0,
            frame_outlined: false,
            frame_stroke_width: 2.0,
            frame_stroke_colour: marshmallow.shape_colour,
            shapes: vec![
                CoverShape::new_with_defaults(
                    "Horizontal Line 1".to_string(),
                    ShapeType::HorizontalLine { thickness: 4.0 },
                    marshmallow.shape_colour,
                ),
                CoverShape::new_with_defaults(
                    "Ellipse 1".to_string(),
                    ShapeType::Ellipse {
                        radius_x: 48.0,
                        radius_y: 48.0,
                        lock_aspect: true,
                    },
                    marshmallow.shape_colour,
                ),
            ],
            text_blocks: vec![
                TextBlock::new_author("Lily Persilly", marshmallow.text_colour),
                TextBlock::new_title("An Account\nof a\nSeamstress", marshmallow.text_colour),
            ],
        }
    }
}