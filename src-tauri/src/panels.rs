//! Panel segmentation.
//!
//! Uses a recursive XY-cut: find the widest run of near-empty rows or columns
//! (a gutter), split there, recurse. Compared to contour finding this is
//! deterministic, needs no CV model, and - most importantly - it produces
//! panels already in reading order, because the recursion mirrors how a page is
//! read. Manga splits columns right-to-left, comics left-to-right, and webtoon
//! strips only ever split horizontally.

use crate::model::{PanelAsset, ReadingOrder, SourceFormat};
use crate::util;
use anyhow::{Context, Result};
use image::{GenericImageView, GrayImage, ImageReader};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    fn area(&self) -> u64 {
        self.w as u64 * self.h as u64
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PanelConfig {
    /// Pixels below this are "ink". Pages are mostly white paper.
    pub ink_threshold: u8,
    /// A row/column counts as empty when this fraction or less of it is ink.
    pub empty_line_ratio: f32,
    /// A gutter must be at least this fraction of the region's size.
    pub min_gutter_ratio: f32,
    /// Absolute minimum gutter thickness in pixels.
    pub min_gutter_px: u32,
    /// Drop panels smaller than this fraction of the whole page.
    pub min_panel_area_ratio: f32,
    /// Never split a region below this many pixels on either axis.
    pub min_panel_px: u32,
    pub max_depth: u32,
    /// Panels with less ink than this are blank filler and get dropped.
    pub min_ink_ratio: f32,
}

impl Default for PanelConfig {
    fn default() -> Self {
        Self {
            ink_threshold: 232,
            // Tuned against real scanned manga pages. The gutter sizes matter
            // most: printed gutters are only 5-10px at typical scan resolution,
            // so a ratio much above 0.003 collapses a whole row into one panel,
            // while loosening `empty_line_ratio` past ~0.02 starts splitting
            // panels at internal whitespace instead.
            empty_line_ratio: 0.008,
            min_gutter_ratio: 0.003,
            min_gutter_px: 4,
            min_panel_area_ratio: 0.012,
            min_panel_px: 48,
            max_depth: 7,
            min_ink_ratio: 0.004,
        }
    }
}

impl PanelConfig {
    /// Webtoon strips are drawn edge to edge with generous vertical gutters, and
    /// a single panel can be a small fraction of a very tall image.
    pub fn for_webtoon() -> Self {
        Self {
            min_gutter_px: 10,
            min_panel_area_ratio: 0.0015,
            min_gutter_ratio: 0.002,
            max_depth: 4,
            ..Default::default()
        }
    }
}

/// Analysis surface over a page: a downscaled ink mask.
///
/// Ink counts are computed per sub-region rather than cached per row and column,
/// because the XY-cut always asks about the region it is currently splitting,
/// not about the full page.
struct InkMap {
    /// Per-pixel ink flags, row-major.
    mask: Vec<bool>,
    w: u32,
    h: u32,
    /// Scale factor back to the original image.
    scale: f32,
}

impl InkMap {
    fn build(gray: &GrayImage, cfg: &PanelConfig, max_dim: u32) -> Self {
        let (ow, oh) = gray.dimensions();
        let longest = ow.max(oh);
        let scale = if longest > max_dim {
            max_dim as f32 / longest as f32
        } else {
            1.0
        };
        let w = ((ow as f32 * scale).round() as u32).max(1);
        let h = ((oh as f32 * scale).round() as u32).max(1);
        let small = if scale < 1.0 {
            image::imageops::resize(gray, w, h, image::imageops::FilterType::Triangle)
        } else {
            gray.clone()
        };

        let mut mask = vec![false; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                if small.get_pixel(x, y).0[0] < cfg.ink_threshold {
                    mask[(y * w + x) as usize] = true;
                }
            }
        }
        InkMap {
            mask,
            w,
            h,
            scale: 1.0 / scale.max(f32::EPSILON),
        }
    }

    fn ink_in(&self, r: Rect) -> u64 {
        let mut count = 0u64;
        for y in r.y..(r.y + r.h).min(self.h) {
            let base = y * self.w;
            for x in r.x..(r.x + r.w).min(self.w) {
                if self.mask[(base + x) as usize] {
                    count += 1;
                }
            }
        }
        count
    }

    fn row_ink(&self, r: Rect, y: u32) -> u32 {
        let base = y * self.w;
        let mut count = 0;
        for x in r.x..(r.x + r.w).min(self.w) {
            if self.mask[(base + x) as usize] {
                count += 1;
            }
        }
        count
    }

    fn col_ink(&self, r: Rect, x: u32) -> u32 {
        let mut count = 0;
        for y in r.y..(r.y + r.h).min(self.h) {
            if self.mask[(y * self.w + x) as usize] {
                count += 1;
            }
        }
        count
    }
}

/// A run of consecutive empty lines.
#[derive(Debug, Clone, Copy)]
struct Gap {
    start: u32,
    len: u32,
}

fn find_gaps(values: &[u32], span: u32, cfg: &PanelConfig) -> Vec<Gap> {
    let limit = (span as f32 * cfg.empty_line_ratio).ceil() as u32;
    let mut gaps = Vec::new();
    let mut run_start: Option<u32> = None;
    for (i, v) in values.iter().enumerate() {
        if *v <= limit {
            run_start.get_or_insert(i as u32);
        } else if let Some(start) = run_start.take() {
            gaps.push(Gap {
                start,
                len: i as u32 - start,
            });
        }
    }
    if let Some(start) = run_start {
        gaps.push(Gap {
            start,
            len: values.len() as u32 - start,
        });
    }
    gaps
}

/// Pick the widest interior gap. Gaps touching an edge are margins, not gutters.
fn best_interior_gap(gaps: &[Gap], extent: u32, cfg: &PanelConfig) -> Option<Gap> {
    let min_len = cfg
        .min_gutter_px
        .max((extent as f32 * cfg.min_gutter_ratio).round() as u32)
        .max(1);
    gaps.iter()
        .filter(|g| g.start > 0 && g.start + g.len < extent)
        .filter(|g| g.len >= min_len)
        .filter(|g| {
            // Both sides of the split must be big enough to be a panel.
            g.start >= cfg.min_panel_px && extent - (g.start + g.len) >= cfg.min_panel_px
        })
        .max_by_key(|g| g.len)
        .copied()
}

fn trim_margins(map: &InkMap, r: Rect, cfg: &PanelConfig) -> Rect {
    let mut top = r.y;
    let mut bottom = r.y + r.h;
    let row_limit = (r.w as f32 * cfg.empty_line_ratio).ceil() as u32;
    while top < bottom && map.row_ink(r, top) <= row_limit {
        top += 1;
    }
    while bottom > top && map.row_ink(r, bottom - 1) <= row_limit {
        bottom -= 1;
    }
    let mid = Rect {
        y: top,
        h: bottom.saturating_sub(top),
        ..r
    };
    if mid.h == 0 {
        return mid;
    }
    let mut left = r.x;
    let mut right = r.x + r.w;
    let col_limit = (mid.h as f32 * cfg.empty_line_ratio).ceil() as u32;
    while left < right && map.col_ink(mid, left) <= col_limit {
        left += 1;
    }
    while right > left && map.col_ink(mid, right - 1) <= col_limit {
        right -= 1;
    }
    Rect {
        x: left,
        y: top,
        w: right.saturating_sub(left),
        h: bottom.saturating_sub(top),
    }
}

fn segment(
    map: &InkMap,
    region: Rect,
    depth: u32,
    order: ReadingOrder,
    cfg: &PanelConfig,
    out: &mut Vec<Rect>,
) {
    let region = trim_margins(map, region, cfg);
    if region.w < cfg.min_panel_px || region.h < cfg.min_panel_px {
        if region.w > 0 && region.h > 0 {
            out.push(region);
        }
        return;
    }
    if depth >= cfg.max_depth {
        out.push(region);
        return;
    }

    let vertical_only = matches!(order, ReadingOrder::TopToBottom);

    // Horizontal cut = split into stacked rows.
    let row_values: Vec<u32> = (region.y..region.y + region.h)
        .map(|y| map.row_ink(region, y))
        .collect();
    let row_gap = best_interior_gap(&find_gaps(&row_values, region.w, cfg), region.h, cfg);

    // Vertical cut = split into side-by-side columns.
    let col_gap = if vertical_only {
        None
    } else {
        let col_values: Vec<u32> = (region.x..region.x + region.w)
            .map(|x| map.col_ink(region, x))
            .collect();
        best_interior_gap(&find_gaps(&col_values, region.h, cfg), region.w, cfg)
    };

    // Comic pages are laid out as rows of panels, so a horizontal cut wins ties.
    let prefer_row = match (row_gap, col_gap) {
        (Some(r), Some(c)) => r.len as f32 >= c.len as f32 * 0.8,
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => {
            out.push(region);
            return;
        }
    };

    if prefer_row {
        let gap = row_gap.expect("row gap present");
        let split = region.y + gap.start;
        let top = Rect {
            h: split - region.y,
            ..region
        };
        let bottom = Rect {
            y: split + gap.len,
            h: (region.y + region.h).saturating_sub(split + gap.len),
            ..region
        };
        segment(map, top, depth + 1, order, cfg, out);
        segment(map, bottom, depth + 1, order, cfg, out);
    } else {
        let gap = col_gap.expect("col gap present");
        let split = region.x + gap.start;
        let left = Rect {
            w: split - region.x,
            ..region
        };
        let right = Rect {
            x: split + gap.len,
            w: (region.x + region.w).saturating_sub(split + gap.len),
            ..region
        };
        // Reading order decides which side is visited first.
        let (first, second) = match order {
            ReadingOrder::RightToLeft => (right, left),
            _ => (left, right),
        };
        segment(map, first, depth + 1, order, cfg, out);
        segment(map, second, depth + 1, order, cfg, out);
    }
}

/// Detect panel rectangles on a page, in reading order, in original-image pixels.
pub fn detect_panels(
    image_path: &Path,
    format: SourceFormat,
    cfg: &PanelConfig,
) -> Result<Vec<Rect>> {
    let img = ImageReader::open(image_path)
        .with_context(|| format!("opening {}", image_path.display()))?
        .with_guessed_format()?
        .decode()
        .with_context(|| format!("decoding {}", image_path.display()))?;
    let (ow, oh) = img.dimensions();
    let gray = img.to_luma8();

    // A very tall image is a webtoon strip regardless of what the project says.
    let is_strip = oh as f32 / ow.max(1) as f32 > 2.2;
    let effective_cfg = if is_strip || format == SourceFormat::Manhwa {
        PanelConfig::for_webtoon()
    } else {
        *cfg
    };
    let order = if is_strip {
        ReadingOrder::TopToBottom
    } else {
        format.reading_order()
    };

    // Keep the analysis map small; 1600px is plenty to find gutters.
    let map = InkMap::build(&gray, &effective_cfg, 1600);
    let whole = Rect {
        x: 0,
        y: 0,
        w: map.w,
        h: map.h,
    };

    let page_ink = map.ink_in(whole);
    if page_ink == 0 {
        return Ok(Vec::new());
    }

    let mut raw = Vec::new();
    segment(&map, whole, 0, order, &effective_cfg, &mut raw);

    let page_area = (map.w as u64 * map.h as u64).max(1);
    let mut rects: Vec<Rect> = raw
        .into_iter()
        .filter(|r| r.w >= 8 && r.h >= 8)
        .filter(|r| {
            (r.area() as f32 / page_area as f32) >= effective_cfg.min_panel_area_ratio
        })
        .filter(|r| {
            // Drop panels that are basically blank paper.
            let ink = map.ink_in(*r) as f32;
            ink / r.area().max(1) as f32 >= effective_cfg.min_ink_ratio
        })
        .collect();

    // If the cut found nothing usable, treat the whole page as one panel.
    if rects.is_empty() {
        rects.push(trim_margins(&map, whole, &effective_cfg));
    }

    // Scale back to original pixels with a small bleed so borders are not clipped.
    let bleed = 3.0;
    let scaled = rects
        .into_iter()
        .map(|r| {
            let x = ((r.x as f32 * map.scale) - bleed).max(0.0) as u32;
            let y = ((r.y as f32 * map.scale) - bleed).max(0.0) as u32;
            let w = (((r.w as f32 * map.scale) + bleed * 2.0) as u32).min(ow.saturating_sub(x));
            let h = (((r.h as f32 * map.scale) + bleed * 2.0) as u32).min(oh.saturating_sub(y));
            Rect { x, y, w, h }
        })
        .filter(|r| r.w > 0 && r.h > 0)
        .collect();

    Ok(scaled)
}

/// Detect panels and write each one out as a PNG next to the page.
pub fn extract_panels_to_disk(
    image_path: &Path,
    out_dir: &Path,
    page_index: usize,
    format: SourceFormat,
    cfg: &PanelConfig,
) -> Result<Vec<PanelAsset>> {
    let rects = detect_panels(image_path, format, cfg)?;
    if rects.is_empty() {
        return Ok(Vec::new());
    }
    let img = ImageReader::open(image_path)?
        .with_guessed_format()?
        .decode()?;
    let (ow, oh) = img.dimensions();
    let page_area = (ow as f32 * oh as f32).max(1.0);

    std::fs::create_dir_all(out_dir)?;
    let mut assets = Vec::with_capacity(rects.len());
    for (i, r) in rects.iter().enumerate() {
        let cropped = img.crop_imm(r.x, r.y, r.w, r.h);
        let name = format!("p{page_index:05}-{i:02}.png");
        let path: PathBuf = out_dir.join(&name);
        cropped
            .save(&path)
            .with_context(|| format!("writing panel {}", path.display()))?;
        assets.push(PanelAsset {
            id: util::new_id("panel"),
            index: i,
            path: path.to_string_lossy().to_string(),
            x: r.x,
            y: r.y,
            width: r.w,
            height: r.h,
            area_ratio: (r.w as f32 * r.h as f32) / page_area,
        });
    }
    Ok(assets)
}

/// Downscale an image for vision calls and return JPEG bytes.
/// Vision APIs bill by pixel area, so keeping this tight matters for cost.
pub fn vision_bytes(path: &Path, max_dim: u32) -> Result<Vec<u8>> {
    let img = ImageReader::open(path)
        .with_context(|| format!("opening {}", path.display()))?
        .with_guessed_format()?
        .decode()?;
    let (w, h) = img.dimensions();
    let longest = w.max(h);
    let resized = if longest > max_dim {
        let scale = max_dim as f32 / longest as f32;
        img.resize(
            ((w as f32 * scale) as u32).max(1),
            ((h as f32 * scale) as u32).max(1),
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        img
    };
    let rgb = resized.to_rgb8();
    let mut buf = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 82);
    encoder.encode(
        rgb.as_raw(),
        rgb.width(),
        rgb.height(),
        image::ExtendedColorType::Rgb8,
    )?;
    Ok(buf)
}

/// Write a downscaled copy to `dest` and return its dimensions.
pub fn write_vision_copy(src: &Path, dest: &Path, max_dim: u32) -> Result<(u32, u32)> {
    let bytes = vision_bytes(src, max_dim)?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(dest, &bytes)?;
    let img = ImageReader::open(src)?.with_guessed_format()?.decode()?;
    Ok(img.dimensions())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Luma, RgbImage};

    fn synthetic_page(path: &Path) {
        // Four panels in a 2x2 grid separated by white gutters.
        let mut img = RgbImage::from_pixel(600, 800, image::Rgb([255, 255, 255]));
        let blocks = [(20u32, 20u32), (320, 20), (20, 420), (320, 420)];
        for (bx, by) in blocks {
            for y in by..by + 340 {
                for x in bx..bx + 260 {
                    img.put_pixel(x, y, image::Rgb([30, 30, 30]));
                }
            }
        }
        img.save(path).unwrap();
    }

    #[test]
    fn xy_cut_finds_a_two_by_two_grid() {
        let dir = std::env::temp_dir().join("recap_panel_test");
        std::fs::create_dir_all(&dir).unwrap();
        let page = dir.join("page.png");
        synthetic_page(&page);
        let rects = detect_panels(&page, SourceFormat::Manga, &PanelConfig::default()).unwrap();
        assert_eq!(rects.len(), 4, "expected 4 panels, got {rects:?}");
        // Manga reads right to left, so the first panel is the top-right one.
        assert!(rects[0].x > rects[1].x, "first panel should be right of second");
        assert!(rects[0].y < rects[2].y, "top row should come before bottom row");
    }

    #[test]
    fn blank_page_yields_no_panels() {
        let dir = std::env::temp_dir().join("recap_panel_test");
        std::fs::create_dir_all(&dir).unwrap();
        let page = dir.join("blank.png");
        image::GrayImage::from_pixel(400, 400, Luma([255])).save(&page).unwrap();
        let rects = detect_panels(&page, SourceFormat::Manga, &PanelConfig::default()).unwrap();
        assert!(rects.is_empty());
    }
}
