//! Diagnostic: sweep panel-detection parameters over a page and report what
//! each combination finds. Used to pick the shipped defaults.
//! Run with: cargo run --example panel_probe -- <image path> [manga|manhwa|comic]

use recap_studio_lib::model::SourceFormat;
use recap_studio_lib::panels::{detect_panels, PanelConfig};

fn main() {
    let path = std::env::args().nth(1).expect("usage: panel_probe <image> [format]");
    let format = match std::env::args().nth(2).as_deref() {
        Some("manhwa") => SourceFormat::Manhwa,
        Some("comic") => SourceFormat::Comic,
        _ => SourceFormat::Manga,
    };

    let img = image::ImageReader::open(&path)
        .unwrap()
        .with_guessed_format()
        .unwrap()
        .decode()
        .unwrap();
    let (w, h) = image::GenericImageView::dimensions(&img);
    println!("{path} -> {w}x{h}, format {format:?}\n");

    println!(
        "{:>9} {:>9} {:>8} {:>6}  panels",
        "emptyTol", "gutterPx", "gutterPct", "minArea"
    );
    for empty_line_ratio in [0.008f32, 0.015, 0.025, 0.04] {
        for min_gutter_px in [4u32, 6, 9] {
            for min_gutter_ratio in [0.003f32, 0.008, 0.012] {
                let cfg = PanelConfig {
                    empty_line_ratio,
                    min_gutter_px,
                    min_gutter_ratio,
                    ..PanelConfig::default()
                };
                let rects = detect_panels(std::path::Path::new(&path), format, &cfg).unwrap();
                println!(
                    "{empty_line_ratio:>9.3} {min_gutter_px:>9} {min_gutter_ratio:>8.3} {:>6.3}  {:>2}  {}",
                    cfg.min_panel_area_ratio,
                    rects.len(),
                    rects
                        .iter()
                        .map(|r| format!("{}x{}", r.w, r.h))
                        .collect::<Vec<_>>()
                        .join(" ")
                );
            }
        }
    }
}
