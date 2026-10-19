#  CoverBuilder Studio

A minimalist, geometric book cover design studio written in Rust using `eframe` / `egui`, inspired by classic modernist and Haruki Murakami / Vintage International book covers.

##  Features
- **Interactive Canvas:** 25%–500% zoom, panning, and composition overlays (Rule of Thirds, Golden Ratio, Print Bleed).
- **Geometric Shape Engine:** Circles/Ellipses, Half-Ellipses/Domes, Polygons/Stars, Triangles, Rectangles, and Lines.
- **Customisable Edge Curves:** Bend edges inward or outward using Quadratic Arch, Sine/Bell Wave, S-Curve, Sharp V-Peak, or Circular Arc functions.
- **Smart Snapping & Outer Margins:** Snap shapes to centres and edges, or hold `Ctrl` whilst dragging to snap using a customisable outer alignment margin.
- **Rich Typography:** Unlimited text layers, local system font discovery, letter-spacing (tracking), curved arch text, banner boxes, outlines, and drop shadows.
- **SVG Import & Export:** Import external `.svg` shapes, and export finished covers to crisp vector `.svg` or high-resolution `.png`.

##  Installation & Running

### From Source (Rust / Cargo)
```bash
git clone https://github.com/YOUR_USERNAME/coverbuilder.git
cd coverbuilder
cargo run --release
