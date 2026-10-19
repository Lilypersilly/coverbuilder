#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod canvas;
mod export_io;
mod models;

use canvas::{render_interactive_canvas, DragTarget};
use eframe::egui;
use export_io::{
    encode_rgba_to_png, ensure_font_loaded, load_custom_palettes, load_system_fonts,
    open_native_file_dialog, parse_svg_content_to_contours, save_custom_palettes,
    scan_saved_projects,
};
use models::{
    AddShapeKind, BookCover, ColourPalette, CompositionOverlay, CoverShape, CurveFunction,
    InstalledFont, ShapeType, TextAlign, TextBlock, TextCase,
};
use std::sync::{Arc, Mutex};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1360.0, 860.0]),
        ..Default::default()
    };

    eframe::run_native(
        "CoverBuilder Studio",
        options,
        Box::new(|cc| Ok(Box::new(CoverBuilderApp::new(cc)))),
    )
}

struct CoverBuilderApp {
    cover: BookCover,
    undo_stack: Vec<BookCover>,
    redo_stack: Vec<BookCover>,
    pre_drag_snapshot: Option<BookCover>,

    installed_fonts: Vec<InstalledFont>,
    loaded_font_indices: std::collections::HashSet<usize>,
    egui_font_defs: egui::FontDefinitions,

    project_name: String,
    saved_projects: Vec<String>,
    selected_project_index: usize,

    palettes: Vec<ColourPalette>,
    selected_palette_index: usize,
    new_palette_name: String,
    font_search: String,

    selected_add_shape: AddShapeKind,
    svg_import_path: String,
    svg_raw_paste: String,

    selected_shape: Option<usize>,
    selected_text: Option<usize>,
    dragging_target: Option<DragTarget>,
    drag_raw_offset: egui::Vec2,

    // Canvas Zoom & Pan
    canvas_zoom: f32,
    canvas_pan: egui::Vec2,
    last_cover_screen_rect: Option<egui::Rect>,
    captured_screenshot: Arc<Mutex<Option<Arc<egui::ColorImage>>>>,

    // Canvas Settings
    enable_snapping: bool,
    show_all_outer_guides: bool,
    anti_aliasing: bool,
    composition_overlay: CompositionOverlay,
    status_message: String,
}

impl CoverBuilderApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.tessellation_options_mut(|opts| {
            opts.feathering = true;
        });

        let installed_fonts = load_system_fonts(&cc.egui_ctx);
        let palettes = load_custom_palettes();
        Self {
            cover: BookCover::default(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            pre_drag_snapshot: None,
            installed_fonts,
            loaded_font_indices: std::collections::HashSet::new(),
            egui_font_defs: egui::FontDefinitions::default(),
            project_name: "my_cover".to_string(),
            saved_projects: scan_saved_projects(),
            selected_project_index: 0,
            palettes,
            selected_palette_index: 0,
            new_palette_name: "My Custom Palette".to_string(),
            font_search: String::new(),
            selected_add_shape: AddShapeKind::Ellipse,
            svg_import_path: "icon.svg".to_string(),
            svg_raw_paste: String::new(),
            selected_shape: None,
            selected_text: None,
            dragging_target: None,
            drag_raw_offset: egui::Vec2::ZERO,
            canvas_zoom: 1.0,
            canvas_pan: egui::Vec2::ZERO,
            last_cover_screen_rect: None,
            captured_screenshot: Arc::new(Mutex::new(None)),
            enable_snapping: true,
            show_all_outer_guides: true,
            anti_aliasing: true,
            composition_overlay: CompositionOverlay::None,
            status_message: String::new(),
        }
    }

    fn push_undo_snapshot(&mut self, snapshot: BookCover) {
        if self.undo_stack.last() != Some(&snapshot) {
            if self.undo_stack.len() >= 50 {
                self.undo_stack.remove(0);
            }
            self.undo_stack.push(snapshot);
            self.redo_stack.clear();
        }
    }

    fn undo(&mut self) {
        if let Some(prev) = self.undo_stack.pop() {
            let current = std::mem::replace(&mut self.cover, prev);
            self.redo_stack.push(current);
            self.status_message = "↩ Undid last action.".to_string();
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            let current = std::mem::replace(&mut self.cover, next);
            self.undo_stack.push(current);
            self.status_message = "↪ Redid action.".to_string();
        }
    }

    fn next_shape_name(&self, prefix: &str) -> String {
        let count = self
            .cover
            .shapes
            .iter()
            .filter(|s| s.name.starts_with(prefix))
            .count();
        format!("{} {}", prefix, count + 1)
    }

    fn apply_palette(&mut self, idx: usize) {
        if let Some(preset) = self.palettes.get(idx).cloned() {
            let snap = self.cover.clone();
            self.push_undo_snapshot(snap);
            self.cover.bg_colour = preset.bg_colour;
            self.cover.inner_colour = preset.inner_colour;
            self.cover.default_shape_colour = preset.shape_colour;
            self.cover.frame_stroke_colour = preset.shape_colour;
            for shape in &mut self.cover.shapes {
                shape.colour = preset.shape_colour;
                shape.stroke_colour = preset.shape_colour;
            }
            for tb in &mut self.cover.text_blocks {
                tb.colour = preset.text_colour;
            }
            self.status_message = format!("🎨 Applied palette: {}", preset.name);
        }
    }
}

impl eframe::App for CoverBuilderApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // On-demand font loader: loads the .ttf/.otf file only for fonts actively selected!
        for tb in &self.cover.text_blocks {
            ensure_font_loaded(
                ui.ctx(),
                &self.installed_fonts,
                &mut self.loaded_font_indices,
                &mut self.egui_font_defs,
                tb.font_index,
            );
        }

        // Process captured screenshot for High-Res PNG Export
        let taken_img = self
            .captured_screenshot
            .lock()
            .ok()
            .and_then(|mut g| g.take());
        if let Some(img) = taken_img {
            let ppp = ui.ctx().pixels_per_point();
            let crop_rect = self
                .last_cover_screen_rect
                .unwrap_or(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(380.0, 560.0),
                ));
            let min_x = ((crop_rect.min.x * ppp).round() as usize).min(img.width());
            let min_y = ((crop_rect.min.y * ppp).round() as usize).min(img.height());
            let max_x = ((crop_rect.max.x * ppp).round() as usize).min(img.width());
            let max_y = ((crop_rect.max.y * ppp).round() as usize).min(img.height());

            if max_x > min_x && max_y > min_y {
                let w = (max_x - min_x) as u32;
                let h = (max_y - min_y) as u32;
                let mut rgba = Vec::with_capacity((w * h * 4) as usize);
                for y in min_y..max_y {
                    for x in min_x..max_x {
                        let c = img[(x, y)];
                        rgba.extend_from_slice(&[c.r(), c.g(), c.b(), c.a()]);
                    }
                }
                let clean = if self.project_name.trim().is_empty() {
                    "cover".to_string()
                } else {
                    self.project_name
                        .trim()
                        .trim_end_matches(".cbproj")
                        .to_string()
                };
                let png_file = format!("{}.png", clean);
                let png_bytes = encode_rgba_to_png(w, h, &rgba);
                match std::fs::write(&png_file, png_bytes) {
                    Ok(_) => {
                        self.status_message =
                            format!("✔ Exported crisp {} ({}×{} px)!", png_file, w, h);
                    }
                    Err(e) => {
                        self.status_message = format!("❌ PNG export failed: {}", e);
                    }
                }
            }
        }

        // Global Keyboard Shortcuts (Undo / Redo & Arrow Key Nudging)
        if !ui.ctx().egui_wants_keyboard_input() {
            let mut do_undo = false;
            let mut do_redo = false;
            let mut nudge = egui::Vec2::ZERO;

            ui.input(|i| {
                if i.modifiers.command && i.key_pressed(egui::Key::Z) {
                    if i.modifiers.shift {
                        do_redo = true;
                    } else {
                        do_undo = true;
                    }
                }
                if i.modifiers.command && i.key_pressed(egui::Key::Y) {
                    do_redo = true;
                }
                let step = if i.modifiers.shift { 10.0 } else { 1.0 };
                if i.key_pressed(egui::Key::ArrowLeft) {
                    nudge.x -= step;
                }
                if i.key_pressed(egui::Key::ArrowRight) {
                    nudge.x += step;
                }
                if i.key_pressed(egui::Key::ArrowUp) {
                    nudge.y -= step;
                }
                if i.key_pressed(egui::Key::ArrowDown) {
                    nudge.y += step;
                }
            });

            if do_undo {
                self.undo();
            } else if do_redo {
                self.redo();
            } else if nudge != egui::Vec2::ZERO {
                if let Some(s_idx) = self.selected_shape {
                    let snap = self.cover.clone();
                    if let Some(shape) = self.cover.shapes.get_mut(s_idx) {
                        if !shape.locked {
                            shape.x_offset += nudge.x;
                            shape.y_offset += nudge.y;
                            self.push_undo_snapshot(snap);
                        }
                    }
                } else if let Some(t_idx) = self.selected_text {
                    let snap = self.cover.clone();
                    if let Some(tb) = self.cover.text_blocks.get_mut(t_idx) {
                        if !tb.locked {
                            tb.x_offset += nudge.x;
                            tb.y_pos += nudge.y;
                            self.push_undo_snapshot(snap);
                        }
                    }
                }
            }
        }

        // ====================================================================
        // 1. LEFT PANEL: PROJECT, PALETTES, COVER & WINDOW SETUP, TYPOGRAPHY
        // ====================================================================
        egui::Panel::left("left_studio_panel")
            .default_size(360.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("CoverBuilder Studio");

                    // --- PROJECT MANAGER & EXPORT ---
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            ui.label("Project Name:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.project_name)
                                    .desired_width(145.0),
                            );
                            if ui
                                .button("💾 Save")
                                .on_hover_text("Save to <Project Name>.cbproj")
                                .clicked()
                            {
                                let clean = if self.project_name.trim().is_empty() {
                                    "project".to_string()
                                } else {
                                    self.project_name
                                        .trim()
                                        .trim_end_matches(".cbproj")
                                        .to_string()
                                };
                                let filename = format!("{}.cbproj", clean);
                                match self.cover.save_project(&filename) {
                                    Ok(_) => {
                                        self.saved_projects = scan_saved_projects();
                                        if let Some(pos) = self
                                            .saved_projects
                                            .iter()
                                            .position(|p| p == &filename)
                                        {
                                            self.selected_project_index = pos;
                                        }
                                        self.status_message =
                                            format!("✔ Saved project to {}!", filename);
                                    }
                                    Err(err) => {
                                        self.status_message = format!("❌ Save error: {}", err);
                                    }
                                }
                            }
                        });

                        let current_proj = self
                            .saved_projects
                            .get(self.selected_project_index)
                            .cloned()
                            .unwrap_or_else(|| "project.cbproj".to_string());

                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt("saved_projects_dropdown")
                                .selected_text(&current_proj)
                                .width(175.0)
                                .show_ui(ui, |ui| {
                                    for (idx, proj_file) in self.saved_projects.iter().enumerate() {
                                        ui.selectable_value(
                                            &mut self.selected_project_index,
                                            idx,
                                            proj_file,
                                        );
                                    }
                                });

                            if ui
                                .button("📂 Load")
                                .on_hover_text("Load selected project from drop-down")
                                .clicked()
                            {
                                match BookCover::load_project(&current_proj) {
                                    Ok(loaded) => {
                                        let snap = self.cover.clone();
                                        self.push_undo_snapshot(snap);
                                        self.cover = loaded;
                                        self.project_name = current_proj
                                            .trim_end_matches(".cbproj")
                                            .to_string();
                                        self.status_message =
                                            format!("✔ Loaded {}!", current_proj);
                                    }
                                    Err(err) => {
                                        self.status_message = format!("❌ Load error: {}", err);
                                    }
                                }
                            }

                            if ui
                                .button("🗂 Browse...")
                                .on_hover_text("Open File Explorer to load any .cbproj file")
                                .clicked()
                            {
                                if let Some(path) = open_native_file_dialog(
                                    "Open CoverBuilder Project",
                                    false,
                                    "cbproj",
                                ) {
                                    match BookCover::load_project(&path) {
                                        Ok(loaded) => {
                                            let snap = self.cover.clone();
                                            self.push_undo_snapshot(snap);
                                            self.cover = loaded;
                                            if let Some(stem) = std::path::Path::new(&path)
                                                .file_stem()
                                                .and_then(|s| s.to_str())
                                            {
                                                self.project_name = stem.to_string();
                                            }
                                            self.saved_projects = scan_saved_projects();
                                            self.status_message = format!("✔ Loaded {}!", path);
                                        }
                                        Err(err) => {
                                            self.status_message =
                                                format!("❌ Load error: {}", err);
                                        }
                                    }
                                }
                            }
                        });

                        ui.horizontal_wrapped(|ui| {
                            if ui.button("💾 Export SVG").clicked() {
                                let clean = if self.project_name.trim().is_empty() {
                                    "cover".to_string()
                                } else {
                                    self.project_name
                                        .trim()
                                        .trim_end_matches(".cbproj")
                                        .to_string()
                                };
                                let svg_file = format!("{}.svg", clean);
                                let svg_data = self.cover.to_svg(&self.installed_fonts);
                                match std::fs::write(&svg_file, svg_data) {
                                    Ok(_) => {
                                        self.status_message =
                                            format!("✔ Saved vector {}!", svg_file);
                                    }
                                    Err(err) => {
                                        self.status_message = format!("❌ Error: {}", err);
                                    }
                                }
                            }
                            if ui.button("🖼 Export PNG").clicked() {
                                let slot = Arc::clone(&self.captured_screenshot);
                                let ctx_clone = ui.ctx().clone();
                                ui.request_screenshot(move |image| {
                                    if let Ok(mut g) = slot.lock() {
                                        *g = Some(image);
                                    }
                                    ctx_clone.request_repaint();
                                });
                            }
                            if ui
                                .button("🔤 + Load Font File")
                                .on_hover_text("Browse for a custom .ttf or .otf font file")
                                .clicked()
                            {
                                if let Some(path_str) = open_native_file_dialog(
                                    "Select Font (.ttf / .otf)",
                                    false,
                                    "ttf",
                                ) {
                                    let path = std::path::PathBuf::from(&path_str);
                                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                                        let name = stem.replace(['-', '_'], " ");
                                        let fam = egui::FontFamily::Name(name.clone().into());
                                        self.installed_fonts.push(InstalledFont {
                                            display_name: name.clone(),
                                            egui_family: fam,
                                            svg_family: format!("'{}', sans-serif", name),
                                            file_path: Some(path),
                                        });
                                        self.status_message = format!("✔ Added font '{}'!", name);
                                    }
                                }
                            }
                        });
                    });

                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!self.undo_stack.is_empty(), egui::Button::new("↩ Undo"))
                            .clicked()
                        {
                            self.undo();
                        }
                        if ui
                            .add_enabled(!self.redo_stack.is_empty(), egui::Button::new("↪ Redo"))
                            .clicked()
                        {
                            self.redo();
                        }
                        ui.checkbox(&mut self.anti_aliasing, "✨ Anti-Aliasing");
                    });

                    if !self.status_message.is_empty() {
                        ui.colored_label(
                            egui::Color32::from_rgb(130, 220, 150),
                            &self.status_message,
                        );
                    }
                    ui.separator();

                    // --- COLOUR PALETTES (DROP-DOWN SELECTOR) ---
                    egui::CollapsingHeader::new("🎨 Colour Palettes & Gradients")
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.label("Select Preset or Custom Palette:");
                            let current_pal_name = self
                                .palettes
                                .get(self.selected_palette_index)
                                .map(|p| p.name.clone())
                                .unwrap_or_else(|| "Select Palette".to_string());

                            let mut trigger_apply = None;
                            ui.horizontal(|ui| {
                                egui::ComboBox::from_id_salt("palette_dropdown")
                                    .selected_text(current_pal_name)
                                    .width(210.0)
                                    .show_ui(ui, |ui| {
                                        for (idx, preset) in self.palettes.iter().enumerate() {
                                            if ui
                                                .selectable_value(
                                                    &mut self.selected_palette_index,
                                                    idx,
                                                    &preset.name,
                                                )
                                                .clicked()
                                            {
                                                trigger_apply = Some(idx);
                                            }
                                        }
                                    });

                                if ui.button("Apply").clicked() {
                                    trigger_apply = Some(self.selected_palette_index);
                                }

                                let is_custom = self
                                    .palettes
                                    .get(self.selected_palette_index)
                                    .map(|p| !p.built_in)
                                    .unwrap_or(false);
                                if is_custom
                                    && ui
                                        .button("🗑")
                                        .on_hover_text("Delete saved custom palette")
                                        .clicked()
                                {
                                    self.palettes.remove(self.selected_palette_index);
                                    self.selected_palette_index = 0;
                                    let _ = save_custom_palettes(&self.palettes);
                                    self.status_message =
                                        "🗑 Deleted custom colour palette.".to_string();
                                }
                            });

                            if let Some(idx) = trigger_apply {
                                self.apply_palette(idx);
                            }

                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.new_palette_name)
                                        .desired_width(175.0),
                                );
                                if ui.button("💾 Save Palette").clicked() {
                                    let name = if self.new_palette_name.trim().is_empty() {
                                        "Custom Palette".to_string()
                                    } else {
                                        self.new_palette_name.trim().to_string()
                                    };
                                    let first_shape_col = self
                                        .cover
                                        .shapes
                                        .first()
                                        .map(|s| s.colour)
                                        .unwrap_or(self.cover.default_shape_colour);
                                    let first_text_col = self
                                        .cover
                                        .text_blocks
                                        .first()
                                        .map(|t| t.colour)
                                        .unwrap_or(egui::Color32::WHITE);
                                    self.palettes.push(ColourPalette {
                                        name: format!("🎨 {}", name),
                                        bg_colour: self.cover.bg_colour,
                                        inner_colour: self.cover.inner_colour,
                                        text_colour: first_text_col,
                                        shape_colour: first_shape_col,
                                        built_in: false,
                                    });
                                    self.selected_palette_index = self.palettes.len() - 1;
                                    let _ = save_custom_palettes(&self.palettes);
                                    self.status_message =
                                        "✔ Saved custom colour palette!".to_string();
                                }
                            });

                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.color_edit_button_srgba(&mut self.cover.bg_colour);
                                ui.label("Outer Background");
                                ui.checkbox(&mut self.cover.bg_gradient, "Gradient");
                                if self.cover.bg_gradient {
                                    ui.color_edit_button_srgba(&mut self.cover.bg_colour_bottom);
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.color_edit_button_srgba(&mut self.cover.inner_colour);
                                ui.label("Inner Window");
                                ui.checkbox(&mut self.cover.inner_gradient, "Gradient");
                                if self.cover.inner_gradient {
                                    ui.color_edit_button_srgba(&mut self.cover.inner_colour_bottom);
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.color_edit_button_srgba(&mut self.cover.default_shape_colour);
                                ui.label("Default New Shape Colour");
                            });
                            ui.add(
                                egui::Slider::new(&mut self.cover.paper_grain, 0.0..=1.0)
                                    .text("Vintage Paper Grain"),
                            );
                        });

                    ui.separator();

                    // --- COVER DIMENSIONS & INNER WINDOW ---
                    egui::CollapsingHeader::new("📐 Cover Trim & Shape Window")
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.label("Cover Trim Size Presets:");
                            ui.horizontal_wrapped(|ui| {
                                if ui.small_button("Standard (380×560)").clicked() {
                                    self.cover.cover_width = 380.0;
                                    self.cover.cover_height = 560.0;
                                }
                                if ui.small_button("Penguin A-Format (350×560)").clicked() {
                                    self.cover.cover_width = 350.0;
                                    self.cover.cover_height = 560.0;
                                }
                                if ui.small_button("6×9 Trade (380×570)").clicked() {
                                    self.cover.cover_width = 380.0;
                                    self.cover.cover_height = 570.0;
                                }
                                if ui.small_button("Audiobook Square (460×460)").clicked() {
                                    self.cover.cover_width = 460.0;
                                    self.cover.cover_height = 460.0;
                                }
                            });

                            ui.add(
                                egui::Slider::new(&mut self.cover.cover_width, 200.0..=800.0)
                                    .text("Cover Width"),
                            );
                            ui.add(
                                egui::Slider::new(&mut self.cover.cover_height, 200.0..=1000.0)
                                    .text("Cover Height"),
                            );
                            ui.separator();

                            ui.add(
                                egui::Slider::new(&mut self.cover.frame_width, 20.0..=700.0)
                                    .clamping(egui::SliderClamping::Never)
                                    .text("Window Width"),
                            );
                            ui.add(
                                egui::Slider::new(&mut self.cover.frame_height, 20.0..=900.0)
                                    .clamping(egui::SliderClamping::Never)
                                    .text("Window Height"),
                            );
                            ui.add(
                                egui::Slider::new(&mut self.cover.frame_x_offset, -250.0..=250.0)
                                    .clamping(egui::SliderClamping::Never)
                                    .text("Window X Offset"),
                            );
                            ui.add(
                                egui::Slider::new(&mut self.cover.frame_y_offset, 0.0..=700.0)
                                    .clamping(egui::SliderClamping::Never)
                                    .text("Window Top Y"),
                            );
                            ui.add(
                                egui::Slider::new(&mut self.cover.frame_rounding, 0.0..=250.0)
                                    .clamping(egui::SliderClamping::Never)
                                    .text("Corner Rounding"),
                            );

                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.cover.frame_outlined, "Window Border");
                                if self.cover.frame_outlined {
                                    ui.color_edit_button_srgba(
                                        &mut self.cover.frame_stroke_colour,
                                    );
                                    ui.add(
                                        egui::DragValue::new(&mut self.cover.frame_stroke_width)
                                            .range(0.5..=40.0)
                                            .speed(0.2)
                                            .prefix("W: "),
                                    );
                                }
                                if ui.small_button("Reset Window").clicked() {
                                    self.cover.frame_width = 324.0;
                                    self.cover.frame_height = 324.0;
                                    self.cover.frame_x_offset = 0.0;
                                    self.cover.frame_y_offset = 75.0;
                                    self.cover.frame_rounding = 4.0;
                                }
                            });
                        });

                    ui.separator();

                    // --- MULTI-TEXT LAYERS & RICH FORMATTING ---
                    egui::CollapsingHeader::new("🔤 Typography & Text Blocks")
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if ui.button("+ Add Text Block").clicked() {
                                    let idx = self.cover.text_blocks.len() + 1;
                                    let col = self
                                        .cover
                                        .text_blocks
                                        .first()
                                        .map(|t| t.colour)
                                        .unwrap_or(egui::Color32::WHITE);
                                    let mut tb = TextBlock::new_author("New Text", col);
                                    tb.name = format!("Text Block {}", idx);
                                    tb.font_size = 24.0;
                                    tb.y_pos = self.cover.cover_height * 0.5;
                                    self.cover.text_blocks.push(tb);
                                    self.selected_text = Some(self.cover.text_blocks.len() - 1);
                                }
                                ui.label("🔍 Font Filter:");
                                ui.text_edit_singleline(&mut self.font_search);
                            });

                            let mut del_tb = None;
                            let mut dup_tb = None;
                            let query = self.font_search.to_lowercase();

                            for (t_idx, tb) in self.cover.text_blocks.iter_mut().enumerate() {
                                let is_sel = self.selected_text == Some(t_idx);
                                ui.group(|ui| {
                                    ui.horizontal(|ui| {
                                        if ui.selectable_label(is_sel, "🎯").clicked() {
                                            self.selected_text = Some(t_idx);
                                            self.selected_shape = None;
                                        }
                                        ui.color_edit_button_srgba(&mut tb.colour);
                                        ui.add(
                                            egui::TextEdit::singleline(&mut tb.name)
                                                .desired_width(115.0),
                                        );
                                        let vis_icon = if tb.visible { "👁" } else { "🚫" };
                                        if ui
                                            .small_button(vis_icon)
                                            .on_hover_text("Toggle visibility")
                                            .clicked()
                                        {
                                            tb.visible = !tb.visible;
                                        }
                                        let lock_icon = if tb.locked { "🔒" } else { "🔓" };
                                        if ui
                                            .small_button(lock_icon)
                                            .on_hover_text("Lock / Unlock position")
                                            .clicked()
                                        {
                                            tb.locked = !tb.locked;
                                        }
                                        if ui.small_button("📋").on_hover_text("Duplicate").clicked()
                                        {
                                            dup_tb = Some(t_idx);
                                        }
                                        if ui.small_button("🗑").on_hover_text("Delete").clicked() {
                                            del_tb = Some(t_idx);
                                        }
                                    });

                                    egui::CollapsingHeader::new(format!("Edit '{}'", tb.name))
                                        .id_salt(format!("tb_header_{}", t_idx))
                                        .default_open(t_idx < 2 || is_sel)
                                        .show(ui, |ui| {
                                            ui.text_edit_multiline(&mut tb.content);

                                            let font_name = self
                                                .installed_fonts
                                                .get(tb.font_index)
                                                .map(|f| f.display_name.as_str())
                                                .unwrap_or("Default");

                                            egui::ComboBox::from_id_salt(format!(
                                                "tb_font_{}",
                                                t_idx
                                            ))
                                            .selected_text(font_name)
                                            .width(220.0)
                                            .show_ui(ui, |ui| {
                                                for (f_i, f_info) in
                                                    self.installed_fonts.iter().enumerate()
                                                {
                                                    if query.is_empty()
                                                        || f_info
                                                            .display_name
                                                            .to_lowercase()
                                                            .contains(&query)
                                                    {
                                                        ui.selectable_value(
                                                            &mut tb.font_index,
                                                            f_i,
                                                            &f_info.display_name,
                                                        );
                                                    }
                                                }
                                            });

                                            ui.horizontal(|ui| {
                                                egui::ComboBox::from_id_salt(format!(
                                                    "tb_case_{}",
                                                    t_idx
                                                ))
                                                .selected_text(tb.case_transform.label())
                                                .width(105.0)
                                                .show_ui(ui, |ui| {
                                                    for c in TextCase::ALL {
                                                        ui.selectable_value(
                                                            &mut tb.case_transform,
                                                            c,
                                                            c.label(),
                                                        );
                                                    }
                                                });

                                                egui::ComboBox::from_id_salt(format!(
                                                    "tb_align_{}",
                                                    t_idx
                                                ))
                                                .selected_text(tb.alignment.label())
                                                .width(80.0)
                                                .show_ui(ui, |ui| {
                                                    for a in TextAlign::ALL {
                                                        ui.selectable_value(
                                                            &mut tb.alignment,
                                                            a,
                                                            a.label(),
                                                        );
                                                    }
                                                });

                                                ui.checkbox(&mut tb.faux_bold, "Bold");
                                            });

                                            ui.add(
                                                egui::Slider::new(&mut tb.font_size, 8.0..=120.0)
                                                    .text("Font Size"),
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut tb.letter_spacing,
                                                    -5.0..=30.0,
                                                )
                                                .text("Letter Spacing"),
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut tb.line_spacing,
                                                    10.0..=120.0,
                                                )
                                                .text("Line Spacing"),
                                            );
                                            ui.add(
                                                egui::Slider::new(&mut tb.x_offset, -250.0..=250.0)
                                                    .text("X Offset"),
                                            );
                                            ui.add(
                                                egui::Slider::new(&mut tb.y_pos, 0.0..=800.0)
                                                    .text("Y Position"),
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut tb.rotation_deg,
                                                    0.0..=360.0,
                                                )
                                                .suffix("°")
                                                .text("Rotation"),
                                            );
                                            ui.add(
                                                egui::Slider::new(
                                                    &mut tb.arch_bend,
                                                    -180.0..=180.0,
                                                )
                                                .suffix("°")
                                                .text("Curved Arch"),
                                            );

                                            ui.horizontal_wrapped(|ui| {
                                                ui.checkbox(&mut tb.outlined, "Outline");
                                                if tb.outlined {
                                                    ui.color_edit_button_srgba(
                                                        &mut tb.outline_colour,
                                                    );
                                                    ui.add(
                                                        egui::DragValue::new(&mut tb.outline_width)
                                                            .range(0.5..=10.0)
                                                            .speed(0.1),
                                                    );
                                                }
                                                ui.checkbox(&mut tb.shadow, "Shadow");
                                                if tb.shadow {
                                                    ui.color_edit_button_srgba(
                                                        &mut tb.shadow_colour,
                                                    );
                                                }
                                                ui.checkbox(&mut tb.banner_box, "Banner Box");
                                                if tb.banner_box {
                                                    ui.color_edit_button_srgba(
                                                        &mut tb.banner_colour,
                                                    );
                                                }
                                                ui.checkbox(
                                                    &mut tb.clip_to_window,
                                                    "✂ Clip to Window",
                                                );
                                            });

                                            ui.horizontal(|ui| {
                                                if ui.small_button("Centre X").clicked() {
                                                    tb.x_offset = 0.0;
                                                }
                                                if ui.small_button("0° Rot").clicked() {
                                                    tb.rotation_deg = 0.0;
                                                }
                                                if ui.small_button("Flat Arch").clicked() {
                                                    tb.arch_bend = 0.0;
                                                }
                                            });
                                        });
                                });
                            }

                            if let Some(i) = del_tb {
                                self.cover.text_blocks.remove(i);
                            } else if let Some(i) = dup_tb {
                                let mut c = self.cover.text_blocks[i].clone();
                                c.name = format!("{} Copy", c.name);
                                c.y_pos += 25.0;
                                self.cover.text_blocks.insert(i + 1, c);
                            }
                        });
                });
            });

        // ====================================================================
        // 2. RIGHT PANEL: DROP-DOWN SHAPE ADDER, SNAPPING & SHAPE LAYERS
        // ====================================================================
        egui::Panel::right("right_layers_panel")
            .default_size(380.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Shapes & Layers");

                    ui.label("Add New Shape Layer:");
                    let new_shape_colour = self.cover.default_shape_colour;
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("add_shape_dropdown")
                            .selected_text(self.selected_add_shape.label())
                            .width(235.0)
                            .show_ui(ui, |ui| {
                                for kind in AddShapeKind::ALL {
                                    ui.selectable_value(
                                        &mut self.selected_add_shape,
                                        kind,
                                        kind.label(),
                                    );
                                }
                            });

                        if ui.button("➕ Add Layer").clicked() {
                            let snap = self.cover.clone();
                            self.push_undo_snapshot(snap);
                            match self.selected_add_shape {
                                AddShapeKind::Ellipse => {
                                    let name = self.next_shape_name("Ellipse");
                                    self.cover.shapes.push(CoverShape::new_with_defaults(
                                        name,
                                        ShapeType::Ellipse {
                                            radius_x: 45.0,
                                            radius_y: 45.0,
                                            lock_aspect: true,
                                        },
                                        new_shape_colour,
                                    ));
                                }
                                AddShapeKind::HalfEllipse => {
                                    let name = self.next_shape_name("Half Ellipse");
                                    self.cover.shapes.push(CoverShape::new_with_defaults(
                                        name,
                                        ShapeType::HalfEllipse {
                                            radius_x: 50.0,
                                            radius_y: 50.0,
                                            lock_aspect: true,
                                        },
                                        new_shape_colour,
                                    ));
                                }
                                AddShapeKind::Rectangle => {
                                    let name = self.next_shape_name("Rectangle");
                                    self.cover.shapes.push(CoverShape::new_with_defaults(
                                        name,
                                        ShapeType::Rectangle {
                                            width: 85.0,
                                            height: 85.0,
                                            curve_func: CurveFunction::QuadraticArch,
                                            link_curves: true,
                                            curve_top: 0.0,
                                            curve_right: 0.0,
                                            curve_bottom: 0.0,
                                            curve_left: 0.0,
                                        },
                                        new_shape_colour,
                                    ));
                                }
                                AddShapeKind::Triangle => {
                                    let name = self.next_shape_name("Triangle");
                                    self.cover.shapes.push(CoverShape::new_with_defaults(
                                        name,
                                        ShapeType::Triangle {
                                            side_a: 90.0,
                                            side_b: 90.0,
                                            side_c: 90.0,
                                            curve_func: CurveFunction::QuadraticArch,
                                            curve_a: 0.0,
                                            curve_b: 0.0,
                                            curve_c: 0.0,
                                        },
                                        new_shape_colour,
                                    ));
                                }
                                AddShapeKind::PolygonStar => {
                                    let name = self.next_shape_name("Polygon");
                                    self.cover.shapes.push(CoverShape::new_with_defaults(
                                        name,
                                        ShapeType::PolygonStar {
                                            sides: 6,
                                            outer_radius: 55.0,
                                            inner_radius: 28.0,
                                            use_star: false,
                                            curve_func: CurveFunction::QuadraticArch,
                                            edge_curve: 0.0,
                                        },
                                        new_shape_colour,
                                    ));
                                }
                                AddShapeKind::HorizontalLine => {
                                    let name = self.next_shape_name("Horizontal Line");
                                    self.cover.shapes.push(CoverShape::new_with_defaults(
                                        name,
                                        ShapeType::HorizontalLine { thickness: 4.0 },
                                        new_shape_colour,
                                    ));
                                }
                                AddShapeKind::VerticalLine => {
                                    let name = self.next_shape_name("Vertical Line");
                                    self.cover.shapes.push(CoverShape::new_with_defaults(
                                        name,
                                        ShapeType::VerticalLine { thickness: 4.0 },
                                        new_shape_colour,
                                    ));
                                }
                                AddShapeKind::ImportSvg => {
                                    let content = if !self.svg_raw_paste.trim().is_empty() {
                                        Ok(self.svg_raw_paste.clone())
                                    } else {
                                        std::fs::read_to_string(&self.svg_import_path)
                                    };
                                    match content {
                                        Ok(raw) => {
                                            let contours = parse_svg_content_to_contours(&raw);
                                            if contours.is_empty() {
                                                self.status_message =
                                                    "❌ No closed vector shapes found in SVG."
                                                        .to_string();
                                            } else {
                                                let name = self.next_shape_name("SVG Shape");
                                                self.cover.shapes.push(
                                                    CoverShape::new_with_defaults(
                                                        name,
                                                        ShapeType::ImportedSvg {
                                                            source_path: self
                                                                .svg_import_path
                                                                .clone(),
                                                            scale_x: 1.0,
                                                            scale_y: 1.0,
                                                            lock_aspect: true,
                                                            contours,
                                                        },
                                                        new_shape_colour,
                                                    ),
                                                );
                                                self.status_message =
                                                    "✔ Imported SVG shape layer!".to_string();
                                            }
                                        }
                                        Err(e) => {
                                            self.status_message =
                                                format!("❌ Could not read SVG file: {}", e);
                                        }
                                    }
                                }
                            }
                            self.selected_shape = Some(self.cover.shapes.len().saturating_sub(1));
                        }
                    });

                    if self.selected_add_shape == AddShapeKind::ImportSvg {
                        ui.group(|ui| {
                            ui.label("SVG File Path (or paste SVG / `<path d=...>` below):");
                            ui.text_edit_singleline(&mut self.svg_import_path);
                            ui.label("Or Paste SVG XML / Path Data:");
                            ui.text_edit_multiline(&mut self.svg_raw_paste);
                        });
                    }

                    ui.separator();

                    ui.checkbox(
                        &mut self.enable_snapping,
                        "🧲 Snap to Centres, Edges & Margins",
                    );
                    ui.label("💡 Hold Ctrl whilst dragging to snap using the shape's Outer Alignment Margin!");
                    ui.checkbox(
                        &mut self.show_all_outer_guides,
                        "👁 Show Outer Alignment Lines While Dragging",
                    );

                    ui.horizontal(|ui| {
                        ui.label("Grid Overlay:");
                        egui::ComboBox::from_id_salt("comp_overlay_combo")
                            .selected_text(self.composition_overlay.label())
                            .width(200.0)
                            .show_ui(ui, |ui| {
                                for ov in CompositionOverlay::ALL {
                                    ui.selectable_value(
                                        &mut self.composition_overlay,
                                        ov,
                                        ov.label(),
                                    );
                                }
                            });
                    });

                    ui.separator();

                    // --- SHAPE LAYERS LIST ---
                    let mut delete_index: Option<usize> = None;
                    let mut move_up_index: Option<usize> = None;
                    let mut move_down_index: Option<usize> = None;
                    let mut duplicate_index: Option<usize> = None;
                    let mut mirror_x_index: Option<usize> = None;
                    let mut mirror_y_index: Option<usize> = None;
                    let num_shapes = self.cover.shapes.len();

                    for (index, shape) in self.cover.shapes.iter_mut().enumerate() {
                        let supports_rot = shape.supports_rotation();
                        let is_selected = self.selected_shape == Some(index);

                        ui.group(|ui| {
                            ui.horizontal(|ui| {
                                if ui.selectable_label(is_selected, "🎯").clicked() {
                                    self.selected_shape = Some(index);
                                    self.selected_text = None;
                                }
                                ui.color_edit_button_srgba(&mut shape.colour);
                                ui.add(
                                    egui::TextEdit::singleline(&mut shape.name)
                                        .desired_width(110.0),
                                );
                                let vis_icon = if shape.visible { "👁" } else { "🚫" };
                                if ui
                                    .small_button(vis_icon)
                                    .on_hover_text("Show / Hide layer")
                                    .clicked()
                                {
                                    shape.visible = !shape.visible;
                                }
                                let lock_icon = if shape.locked { "🔒" } else { "🔓" };
                                if ui
                                    .small_button(lock_icon)
                                    .on_hover_text("Lock / Unlock layer")
                                    .clicked()
                                {
                                    shape.locked = !shape.locked;
                                }
                                if ui
                                    .small_button("⬆")
                                    .on_hover_text("Move backward")
                                    .clicked()
                                    && index > 0
                                {
                                    move_up_index = Some(index);
                                }
                                if ui
                                    .small_button("⬇")
                                    .on_hover_text("Bring forward")
                                    .clicked()
                                    && index + 1 < num_shapes
                                {
                                    move_down_index = Some(index);
                                }
                                if ui
                                    .small_button("📋")
                                    .on_hover_text("Duplicate layer")
                                    .clicked()
                                {
                                    duplicate_index = Some(index);
                                }
                                if ui.small_button("🗑").on_hover_text("Delete layer").clicked() {
                                    delete_index = Some(index);
                                }
                            });

                            egui::CollapsingHeader::new(format!("Properties ({})", shape.name))
                                .id_salt(format!("shape_prop_{}", index))
                                .default_open(is_selected || num_shapes <= 3)
                                .show(ui, |ui| {
                                    if supports_rot {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.checkbox(&mut shape.filled, "Fill");
                                            ui.checkbox(&mut shape.outlined, "Outline");
                                            if shape.outlined {
                                                ui.color_edit_button_srgba(
                                                    &mut shape.stroke_colour,
                                                );
                                                ui.add(
                                                    egui::DragValue::new(&mut shape.stroke_width)
                                                        .range(0.5..=60.0)
                                                        .speed(0.2)
                                                        .prefix("W: "),
                                                );
                                            }
                                            ui.checkbox(
                                                &mut shape.clip_to_window,
                                                "✂ Clip to Window",
                                            );
                                        });
                                    } else {
                                        ui.checkbox(&mut shape.clip_to_window, "✂ Clip to Window");
                                    }

                                    match &mut shape.shape_type {
                                        ShapeType::Ellipse {
                                            radius_x,
                                            radius_y,
                                            lock_aspect,
                                        }
                                        | ShapeType::HalfEllipse {
                                            radius_x,
                                            radius_y,
                                            lock_aspect,
                                        } => {
                                            ui.checkbox(
                                                lock_aspect,
                                                "🔗 Lock Aspect Ratio (Equal Radius)",
                                            );
                                            let prev_rx = *radius_x;
                                            let prev_ry = *radius_y;

                                            ui.add(
                                                egui::Slider::new(radius_x, 1.0..=400.0)
                                                    .clamping(egui::SliderClamping::Never)
                                                    .text("Radius X"),
                                            );
                                            ui.add(
                                                egui::Slider::new(radius_y, 1.0..=400.0)
                                                    .clamping(egui::SliderClamping::Never)
                                                    .text("Radius Y"),
                                            );
                                            *radius_x = radius_x.max(0.5);
                                            *radius_y = radius_y.max(0.5);

                                            if *lock_aspect {
                                                if (*radius_x - prev_rx).abs() > 1e-3 {
                                                    *radius_y = *radius_x;
                                                } else if (*radius_y - prev_ry).abs() > 1e-3 {
                                                    *radius_x = *radius_y;
                                                }
                                            }
                                        }
                                        ShapeType::HorizontalLine { thickness }
                                        | ShapeType::VerticalLine { thickness } => {
                                            ui.add(
                                                egui::Slider::new(thickness, 1.0..=200.0)
                                                    .clamping(egui::SliderClamping::Never)
                                                    .text("Thickness"),
                                            );
                                            *thickness = thickness.max(0.5);
                                        }
                                        ShapeType::Rectangle {
                                            width,
                                            height,
                                            curve_func,
                                            link_curves,
                                            curve_top,
                                            curve_right,
                                            curve_bottom,
                                            curve_left,
                                        } => {
                                            ui.add(
                                                egui::Slider::new(width, 1.0..=500.0)
                                                    .clamping(egui::SliderClamping::Never)
                                                    .text("Width"),
                                            );
                                            ui.add(
                                                egui::Slider::new(height, 1.0..=500.0)
                                                    .clamping(egui::SliderClamping::Never)
                                                    .text("Height"),
                                            );
                                            *width = width.max(0.5);
                                            *height = height.max(0.5);

                                            egui::ComboBox::from_id_salt(format!(
                                                "rect_curve_fn_{}",
                                                index
                                            ))
                                            .selected_text(curve_func.label())
                                            .show_ui(ui, |ui| {
                                                for func in CurveFunction::ALL {
                                                    ui.selectable_value(
                                                        curve_func,
                                                        func,
                                                        func.label(),
                                                    );
                                                }
                                            });

                                            ui.checkbox(link_curves, "🔗 Link All 4 Edge Curves");
                                            if *link_curves {
                                                if ui
                                                    .add(
                                                        egui::Slider::new(
                                                            curve_top,
                                                            -150.0..=150.0,
                                                        )
                                                        .clamping(egui::SliderClamping::Never)
                                                        .text("All Edges Curve"),
                                                    )
                                                    .changed()
                                                {
                                                    *curve_right = *curve_top;
                                                    *curve_bottom = *curve_top;
                                                    *curve_left = *curve_top;
                                                }
                                            } else {
                                                ui.add(
                                                    egui::Slider::new(curve_top, -150.0..=150.0)
                                                        .clamping(egui::SliderClamping::Never)
                                                        .text("Top Curve"),
                                                );
                                                ui.add(
                                                    egui::Slider::new(curve_right, -150.0..=150.0)
                                                        .clamping(egui::SliderClamping::Never)
                                                        .text("Right Curve"),
                                                );
                                                ui.add(
                                                    egui::Slider::new(curve_bottom, -150.0..=150.0)
                                                        .clamping(egui::SliderClamping::Never)
                                                        .text("Bottom Curve"),
                                                );
                                                ui.add(
                                                    egui::Slider::new(curve_left, -150.0..=150.0)
                                                        .clamping(egui::SliderClamping::Never)
                                                        .text("Left Curve"),
                                                );
                                            }
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
                                            ui.add(
                                                egui::Slider::new(side_a, 5.0..=500.0)
                                                    .clamping(egui::SliderClamping::Never)
                                                    .text("Side A Length"),
                                            );
                                            ui.add(
                                                egui::Slider::new(side_b, 5.0..=500.0)
                                                    .clamping(egui::SliderClamping::Never)
                                                    .text("Side B Length"),
                                            );
                                            ui.add(
                                                egui::Slider::new(side_c, 5.0..=500.0)
                                                    .clamping(egui::SliderClamping::Never)
                                                    .text("Side C Length"),
                                            );
                                            *side_a = side_a.max(2.0);
                                            *side_b = side_b.max(2.0);
                                            *side_c = side_c.max(2.0);

                                            let min_c = (*side_a - *side_b).abs() + 1.0;
                                            let max_c = *side_a + *side_b - 1.0;
                                            if *side_c < min_c || *side_c > max_c {
                                                *side_c = side_c.clamp(min_c, max_c.max(min_c));
                                            }

                                            egui::ComboBox::from_id_salt(format!(
                                                "tri_curve_fn_{}",
                                                index
                                            ))
                                            .selected_text(curve_func.label())
                                            .show_ui(ui, |ui| {
                                                for func in CurveFunction::ALL {
                                                    ui.selectable_value(
                                                        curve_func,
                                                        func,
                                                        func.label(),
                                                    );
                                                }
                                            });

                                            ui.add(
                                                egui::Slider::new(curve_a, -150.0..=150.0)
                                                    .clamping(egui::SliderClamping::Never)
                                                    .text("Curve A"),
                                            );
                                            ui.add(
                                                egui::Slider::new(curve_b, -150.0..=150.0)
                                                    .clamping(egui::SliderClamping::Never)
                                                    .text("Curve B"),
                                            );
                                            ui.add(
                                                egui::Slider::new(curve_c, -150.0..=150.0)
                                                    .clamping(egui::SliderClamping::Never)
                                                    .text("Curve C"),
                                            );
                                        }
                                        ShapeType::PolygonStar {
                                            sides,
                                            outer_radius,
                                            inner_radius,
                                            use_star,
                                            curve_func,
                                            edge_curve,
                                        } => {
                                            ui.add(
                                                egui::Slider::new(sides, 3..=24).text("Sides"),
                                            );
                                            ui.add(
                                                egui::Slider::new(outer_radius, 5.0..=400.0)
                                                    .text("Outer Radius"),
                                            );
                                            ui.checkbox(use_star, "⭐ Star Mode (Inner Points)");
                                            if *use_star {
                                                ui.add(
                                                    egui::Slider::new(inner_radius, 2.0..=400.0)
                                                        .text("Inner Radius"),
                                                );
                                            }
                                            egui::ComboBox::from_id_salt(format!(
                                                "poly_curve_fn_{}",
                                                index
                                            ))
                                            .selected_text(curve_func.label())
                                            .show_ui(ui, |ui| {
                                                for func in CurveFunction::ALL {
                                                    ui.selectable_value(
                                                        curve_func,
                                                        func,
                                                        func.label(),
                                                    );
                                                }
                                            });
                                            ui.add(
                                                egui::Slider::new(edge_curve, -100.0..=100.0)
                                                    .text("Edge Curve"),
                                            );
                                        }
                                        ShapeType::ImportedSvg {
                                            scale_x,
                                            scale_y,
                                            lock_aspect,
                                            ..
                                        } => {
                                            ui.checkbox(lock_aspect, "🔗 Lock Aspect Ratio");
                                            let prev_sx = *scale_x;
                                            let prev_sy = *scale_y;
                                            ui.add(
                                                egui::Slider::new(scale_x, 0.1..=6.0)
                                                    .text("Scale X"),
                                            );
                                            ui.add(
                                                egui::Slider::new(scale_y, 0.1..=6.0)
                                                    .text("Scale Y"),
                                            );
                                            if *lock_aspect {
                                                if (*scale_x - prev_sx).abs() > 1e-3 {
                                                    *scale_y = *scale_x;
                                                } else if (*scale_y - prev_sy).abs() > 1e-3 {
                                                    *scale_x = *scale_y;
                                                }
                                            }
                                        }
                                    }

                                    if supports_rot {
                                        ui.add(
                                            egui::Slider::new(&mut shape.rotation_deg, 0.0..=360.0)
                                                .suffix("°")
                                                .text("Rotation"),
                                        );
                                    }

                                    // Secondary Outer Alignment Line Controls (Per-Shape)
                                    ui.checkbox(
                                        &mut shape.enable_outer_align,
                                        "📏 Secondary Outer Alignment Line",
                                    );
                                    if shape.enable_outer_align {
                                        ui.add(
                                            egui::Slider::new(
                                                &mut shape.align_offset,
                                                1.0..=150.0,
                                            )
                                            .clamping(egui::SliderClamping::Never)
                                            .text("Alignment Margin Offset"),
                                        );
                                    }

                                    // Array / Repeat Duplicator Controls
                                    ui.horizontal(|ui| {
                                        ui.label("Repeat:");
                                        ui.add(
                                            egui::DragValue::new(&mut shape.repeat_count)
                                                .range(1..=30),
                                        );
                                        if shape.repeat_count > 1 {
                                            ui.add(
                                                egui::DragValue::new(&mut shape.repeat_step_x)
                                                    .speed(0.5)
                                                    .prefix("dX: "),
                                            );
                                            ui.add(
                                                egui::DragValue::new(&mut shape.repeat_step_y)
                                                    .speed(0.5)
                                                    .prefix("dY: "),
                                            );
                                        }
                                    });

                                    ui.horizontal_wrapped(|ui| {
                                        if ui.small_button("Centre X").clicked() {
                                            shape.x_offset = 0.0;
                                        }
                                        if ui.small_button("Centre Y").clicked() {
                                            shape.y_offset = 0.0;
                                        }
                                        if supports_rot && ui.small_button("0° Rot").clicked() {
                                            shape.rotation_deg = 0.0;
                                        }
                                        if ui
                                            .small_button("🪞 Mirror X")
                                            .on_hover_text(
                                                "Create a horizontally mirrored copy across the centre",
                                            )
                                            .clicked()
                                        {
                                            mirror_x_index = Some(index);
                                        }
                                        if ui
                                            .small_button("🪞 Mirror Y")
                                            .on_hover_text(
                                                "Create a vertically mirrored copy across the centre",
                                            )
                                            .clicked()
                                        {
                                            mirror_y_index = Some(index);
                                        }
                                    });
                                });
                        });
                    }

                    if let Some(i) = delete_index {
                        let snap = self.cover.clone();
                        self.push_undo_snapshot(snap);
                        self.cover.shapes.remove(i);
                    } else if let Some(i) = move_up_index {
                        let snap = self.cover.clone();
                        self.push_undo_snapshot(snap);
                        self.cover.shapes.swap(i, i - 1);
                    } else if let Some(i) = move_down_index {
                        let snap = self.cover.clone();
                        self.push_undo_snapshot(snap);
                        self.cover.shapes.swap(i, i + 1);
                    } else if let Some(i) = duplicate_index {
                        let snap = self.cover.clone();
                        self.push_undo_snapshot(snap);
                        let mut cloned = self.cover.shapes[i].clone();
                        cloned.name = format!("{} Copy", cloned.name);
                        cloned.x_offset += 15.0;
                        cloned.y_offset += 15.0;
                        self.cover.shapes.insert(i + 1, cloned);
                    } else if let Some(i) = mirror_x_index {
                        let snap = self.cover.clone();
                        self.push_undo_snapshot(snap);
                        let mut mirrored = self.cover.shapes[i].clone();
                        mirrored.name = format!("{} (Mirror X)", mirrored.name);
                        mirrored.x_offset = -mirrored.x_offset;
                        if mirrored.supports_rotation() {
                            mirrored.rotation_deg = (360.0 - mirrored.rotation_deg) % 360.0;
                        }
                        self.cover.shapes.insert(i + 1, mirrored);
                    } else if let Some(i) = mirror_y_index {
                        let snap = self.cover.clone();
                        self.push_undo_snapshot(snap);
                        let mut mirrored = self.cover.shapes[i].clone();
                        mirrored.name = format!("{} (Mirror Y)", mirrored.name);
                        mirrored.y_offset = -mirrored.y_offset;
                        if mirrored.supports_rotation() {
                            mirrored.rotation_deg = (180.0 - mirrored.rotation_deg).rem_euclid(360.0);
                        }
                        self.cover.shapes.insert(i + 1, mirrored);
                    }
                });
            });

        // ====================================================================
        // 3. CENTRAL CANVAS PANEL (ZOOM, PAN, DRAG & CTRL OUTER-MARGIN SNAP)
        // ====================================================================
        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(drag_undo_snap) = render_interactive_canvas(
                ui,
                &mut self.cover,
                &self.installed_fonts,
                &mut self.canvas_zoom,
                &mut self.canvas_pan,
                &mut self.selected_shape,
                &mut self.selected_text,
                &mut self.dragging_target,
                &mut self.drag_raw_offset,
                &mut self.pre_drag_snapshot,
                &mut self.last_cover_screen_rect,
                self.enable_snapping,
                self.show_all_outer_guides,
                self.anti_aliasing,
                self.composition_overlay,
            ) {
                self.push_undo_snapshot(drag_undo_snap);
            }
        });
    }
}