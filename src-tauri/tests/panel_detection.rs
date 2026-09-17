//! Panel detection against a real manga page.
//!
//! The fixture is a real scanned page, which is not redistributed with this
//! repository. Drop one at `src-tauri/tests/fixtures/page.jpg` to run this
//! check against it; without it the test skips, so the suite still passes on a
//! clean checkout.

use recap_studio_lib::model::SourceFormat;
use recap_studio_lib::panels::{detect_panels, PanelConfig};
use std::path::PathBuf;

fn fixture() -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/page.jpg");
    path.exists().then_some(path)
}

#[test]
fn finds_panels_on_a_real_manga_page() {
    let Some(page) = fixture() else {
        eprintln!("no fixture at tests/fixtures/page.jpg, skipping");
        return;
    };

    let rects = detect_panels(&page, SourceFormat::Manga, &PanelConfig::default())
        .expect("panel detection should not fail on a valid jpeg");

    // The original Python extractor found six panels on this page, and the
    // shipped thresholds reproduce that. The range allows a little slack for
    // resampling differences while still catching the two failure modes that
    // matter: collapsing a row into one blob, or shattering it into slivers.
    assert!(
        (5..=8).contains(&rects.len()),
        "expected about 6 panels, got {}: {rects:?}",
        rects.len()
    );

    // Manga reads right to left, so the first panel should start on the right
    // half of the page.
    let img = image::ImageReader::open(&page)
        .unwrap()
        .with_guessed_format()
        .unwrap()
        .decode()
        .unwrap();
    let width = image::GenericImageView::dimensions(&img).0;
    assert!(
        rects[0].x + rects[0].w / 2 > width / 2,
        "first panel should sit on the right half for right-to-left reading, got x={} w={} (page width {width})",
        rects[0].x,
        rects[0].w
    );

    // No panel may fall outside the page.
    let (w, h) = image::GenericImageView::dimensions(&img);
    for r in &rects {
        assert!(r.x + r.w <= w && r.y + r.h <= h, "panel out of bounds: {r:?}");
        assert!(r.w > 0 && r.h > 0);
    }
}

#[test]
fn webtoon_mode_only_cuts_horizontally() {
    let Some(page) = fixture() else {
        eprintln!("no fixture at tests/fixtures/page.jpg, skipping");
        return;
    };

    let rects = detect_panels(&page, SourceFormat::Manhwa, &PanelConfig::default()).unwrap();
    assert!(!rects.is_empty());

    // Vertical scroll means every panel is a full-width band, so no two panels
    // should sit side by side.
    let mut sorted = rects.clone();
    sorted.sort_by_key(|r| r.y);
    for pair in sorted.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        assert!(
            b.y >= a.y + a.h / 2,
            "webtoon panels should stack, found overlapping bands {a:?} and {b:?}"
        );
    }
}
