//! Turning whatever the user dropped on the app into an ordered `Source`.
//!
//! Three shapes are supported:
//!   1. A PDF - rasterized in the webview, staged here page by page.
//!   2. A folder of chapter folders - each subfolder becomes a chapter.
//!   3. Loose image files - a single chapter, sorted naturally.

use crate::events::Emitter;
use crate::model::{ChapterRef, PageAsset, Source, SourceFormat};
use crate::panels::{self, PanelConfig};
use crate::project::Paths;
use crate::util;
use anyhow::{anyhow, Context, Result};
use image::ImageReader;
use rayon::prelude::*;
use std::path::{Path, PathBuf};

/// One image queued for ingest, with the chapter it belongs to.
#[derive(Debug, Clone)]
pub struct StagedItem {
    pub src: PathBuf,
    /// Grouping key; every item sharing a key lands in the same chapter.
    pub chapter_key: String,
    pub chapter_label: String,
    pub chapter_number: Option<f32>,
    pub label: String,
}

/// Walk the dropped paths and work out the chapter structure.
pub fn collect(paths: &[PathBuf]) -> Result<(Vec<StagedItem>, String)> {
    let mut items = Vec::new();
    let mut strategy = String::new();

    // Loose files dropped directly all belong to one chapter.
    let loose: Vec<&PathBuf> = paths
        .iter()
        .filter(|p| p.is_file() && util::is_image_file(p))
        .collect();
    if !loose.is_empty() {
        strategy = "Loose images, treated as a single chapter".into();
        for path in loose {
            items.push(StagedItem {
                label: file_label(path),
                src: path.clone(),
                chapter_key: "__loose__".into(),
                chapter_label: "Chapter 1".into(),
                chapter_number: Some(1.0),
            });
        }
    }

    for dir in paths.iter().filter(|p| p.is_dir()) {
        let subdirs = image_bearing_subdirs(dir);
        if subdirs.is_empty() {
            // Flat folder of images: one chapter, named after the folder.
            let label = dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Chapter 1".into());
            let mut images = images_in(dir);
            images.sort_by_key(|p| util::natural_key(&file_label(p)));
            if images.is_empty() {
                continue;
            }
            if strategy.is_empty() {
                strategy = "One folder of images, treated as a single chapter".into();
            }
            for path in images {
                items.push(StagedItem {
                    label: file_label(&path),
                    src: path,
                    chapter_key: dir.to_string_lossy().to_string(),
                    chapter_label: label.clone(),
                    chapter_number: util::parse_chapter_number(&label),
                });
            }
        } else {
            strategy = format!(
                "{} chapter folders detected inside {}",
                subdirs.len(),
                dir.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| dir.display().to_string())
            );
            for sub in subdirs {
                let label = sub
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Chapter".into());
                let mut images = images_in(&sub);
                images.sort_by_key(|p| util::natural_key(&file_label(p)));
                for path in images {
                    items.push(StagedItem {
                        label: file_label(&path),
                        src: path,
                        chapter_key: sub.to_string_lossy().to_string(),
                        chapter_label: label.clone(),
                        chapter_number: util::parse_chapter_number(&label),
                    });
                }
            }
        }
    }

    if items.is_empty() {
        return Err(anyhow!(
            "No images found. Drop a PDF, a folder of chapter folders, or image files \
             (png, jpg, webp, bmp, gif, tiff)."
        ));
    }

    // Order chapters by parsed number when available, otherwise naturally by name.
    order_items(&mut items);
    Ok((items, strategy))
}

fn order_items(items: &mut [StagedItem]) {
    items.sort_by(|a, b| {
        let a_num = a.chapter_number.unwrap_or(f32::MAX);
        let b_num = b.chapter_number.unwrap_or(f32::MAX);
        a_num
            .partial_cmp(&b_num)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| util::natural_key(&a.chapter_label).cmp(&util::natural_key(&b.chapter_label)))
            .then_with(|| util::natural_key(&a.label).cmp(&util::natural_key(&b.label)))
    });
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn images_in(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file() && util::is_image_file(p))
                .collect()
        })
        .unwrap_or_default()
}

/// Direct subdirectories that contain at least one image, sorted naturally.
fn image_bearing_subdirs(dir: &Path) -> Vec<PathBuf> {
    let mut subs: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir() && !images_in(p).is_empty())
                .collect()
        })
        .unwrap_or_default();
    subs.sort_by_key(|p| util::natural_key(&file_label(p)));
    subs
}

/// Copy a staged PDF page into the workspace. Called once per rendered page by
/// the frontend rasterizer.
pub fn stage_pdf_page(root: &Path, ordinal: usize, bytes: &[u8]) -> Result<PathBuf> {
    let staging = Path::new(root).join(".staging");
    std::fs::create_dir_all(&staging)?;
    let path = staging.join(format!("page-{ordinal:05}.png"));
    std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

pub fn staged_pdf_pages(root: &Path) -> Vec<PathBuf> {
    let staging = Path::new(root).join(".staging");
    let mut pages = images_in(&staging);
    pages.sort_by_key(|p| util::natural_key(&file_label(p)));
    pages
}

pub fn clear_staging(root: &Path) {
    let staging = Path::new(root).join(".staging");
    let _ = std::fs::remove_dir_all(staging);
}

/// Build the `Source` for a project: copy pages in, make vision copies, and
/// segment panels.
pub fn build_source(
    root: &Path,
    items: Vec<StagedItem>,
    format: SourceFormat,
    strategy: String,
    vision_max_dim: u32,
    extract_panels: bool,
    emitter: &Emitter,
) -> Result<Source> {
    let paths = Paths::new(root);
    paths.ensure()?;

    let total = items.len();
    emitter.progress("copy", 0, total, format!("Importing {total} pages"));

    // Assign chapter indices in the order the keys first appear.
    let mut chapter_keys: Vec<String> = Vec::new();
    for item in &items {
        if !chapter_keys.contains(&item.chapter_key) {
            chapter_keys.push(item.chapter_key.clone());
        }
    }

    let mut pages: Vec<PageAsset> = Vec::with_capacity(total);
    let mut per_chapter_counter: Vec<usize> = vec![0; chapter_keys.len()];

    for (index, item) in items.iter().enumerate() {
        let chapter_index = chapter_keys
            .iter()
            .position(|k| *k == item.chapter_key)
            .unwrap_or(0);
        let page_in_chapter = per_chapter_counter[chapter_index];
        per_chapter_counter[chapter_index] += 1;

        let ext = item
            .src
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_ascii_lowercase();
        let dest = paths.pages().join(format!("page-{index:05}.{ext}"));
        // A staged PDF page is already inside the workspace; move it instead of copying.
        if item.src.starts_with(paths.root.join(".staging")) {
            // A rename fails across volumes, so fall back to a copy.
            if std::fs::rename(&item.src, &dest).is_err() {
                std::fs::copy(&item.src, &dest)
                    .with_context(|| format!("moving staged page {}", item.label))?;
            }
        } else {
            std::fs::copy(&item.src, &dest)
                .with_context(|| format!("copying {} into the project", item.src.display()))?;
        }

        let vision_path = paths.vision().join(format!("page-{index:05}.jpg"));
        let (w, h) = panels::write_vision_copy(&dest, &vision_path, vision_max_dim)
            .or_else(|err| {
                // A corrupt or unsupported file should skip, not kill the import.
                emitter.warn(format!("Could not read {}: {err}", item.label));
                ImageReader::open(&dest)
                    .and_then(|r| r.with_guessed_format())
                    .ok()
                    .and_then(|r| r.into_dimensions().ok())
                    .map(Ok)
                    .unwrap_or(Err(err))
            })
            .unwrap_or((0, 0));

        pages.push(PageAsset {
            id: util::new_id("page"),
            index,
            chapter_index,
            page_in_chapter,
            path: dest.to_string_lossy().to_string(),
            thumb_path: vision_path.to_string_lossy().to_string(),
            width: w,
            height: h,
            label: item.label.clone(),
            panels: Vec::new(),
        });

        if index % 5 == 0 || index + 1 == total {
            emitter.progress(
                "copy",
                index + 1,
                total,
                format!("Imported {}/{total} pages", index + 1),
            );
        }
    }

    // Panel segmentation is CPU bound and embarrassingly parallel.
    if extract_panels {
        emitter.progress("panels", 0, total, "Finding panels");
        let cfg = PanelConfig::default();
        let panels_dir = paths.panels();
        let counter = std::sync::atomic::AtomicUsize::new(0);
        let results: Vec<(usize, Vec<crate::model::PanelAsset>)> = pages
            .par_iter()
            .map(|page| {
                let found = panels::extract_panels_to_disk(
                    Path::new(&page.path),
                    &panels_dir,
                    page.index,
                    format,
                    &cfg,
                )
                .unwrap_or_else(|err| {
                    tracing::warn!("panel extraction failed for {}: {err}", page.label);
                    Vec::new()
                });
                let done = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                if done % 4 == 0 || done == total {
                    emitter.progress("panels", done, total, format!("Segmented {done}/{total} pages"));
                }
                (page.index, found)
            })
            .collect();

        let mut by_index: std::collections::HashMap<usize, Vec<crate::model::PanelAsset>> =
            results.into_iter().collect();
        for page in pages.iter_mut() {
            if let Some(found) = by_index.remove(&page.index) {
                page.panels = found;
            }
        }
    }

    // Build the chapter table.
    let mut chapters: Vec<ChapterRef> = Vec::with_capacity(chapter_keys.len());
    for (chapter_index, key) in chapter_keys.iter().enumerate() {
        let item = items
            .iter()
            .find(|i| &i.chapter_key == key)
            .expect("chapter key came from items");
        let member_pages: Vec<&PageAsset> = pages
            .iter()
            .filter(|p| p.chapter_index == chapter_index)
            .collect();
        let (Some(first), Some(last)) = (member_pages.first(), member_pages.last()) else {
            continue;
        };
        chapters.push(ChapterRef {
            index: chapter_index,
            number: item.chapter_number.or(Some(chapter_index as f32 + 1.0)),
            title: Some(item.chapter_label.clone()),
            origin: item.chapter_label.clone(),
            first_page: first.index,
            last_page: last.index,
        });
    }

    clear_staging(root);

    let total_panels = pages.iter().map(|p| p.panels.len()).sum();
    emitter.info(format!(
        "Imported {} pages across {} chapter(s), {} panels detected",
        pages.len(),
        chapters.len(),
        total_panels
    ));

    Ok(Source {
        format,
        pages,
        chapters,
        chapter_strategy: strategy,
        total_panels,
    })
}

/// Build staged items for PDF pages already written into `.staging`.
pub fn pdf_items(root: &Path, chapter_label: &str) -> Vec<StagedItem> {
    staged_pdf_pages(root)
        .into_iter()
        .map(|path| StagedItem {
            label: file_label(&path),
            src: path,
            chapter_key: "__pdf__".into(),
            chapter_label: chapter_label.to_string(),
            chapter_number: util::parse_chapter_number(chapter_label),
        })
        .collect()
}
