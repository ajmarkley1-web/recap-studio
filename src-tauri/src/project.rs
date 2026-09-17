//! Project workspaces on disk.
//!
//! Every project is a self-contained folder, so a user can zip one up, move it
//! to another machine and keep working. `project.json` holds the whole model;
//! the sibling folders hold the images and rendered output.
//!
//! ```text
//! <projects_root>/<slug>/
//!   project.json
//!   pages/      full-resolution pages
//!   vision/     downscaled copies sent to vision models
//!   panels/     cropped panels
//!   portraits/  character portraits for the story bible
//!   audio/      narration clips
//!   out/        scripts, exports, recap.mp4
//! ```

use crate::model::{Project, ProjectMeta, ScriptBundle, Source, SourceFormat, StoryBible};
use crate::util;
use anyhow::{anyhow, Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn manifest(&self) -> PathBuf {
        self.root.join("project.json")
    }
    pub fn pages(&self) -> PathBuf {
        self.root.join("pages")
    }
    pub fn vision(&self) -> PathBuf {
        self.root.join("vision")
    }
    pub fn panels(&self) -> PathBuf {
        self.root.join("panels")
    }
    pub fn portraits(&self) -> PathBuf {
        self.root.join("portraits")
    }
    pub fn audio(&self) -> PathBuf {
        self.root.join("audio")
    }
    pub fn out(&self) -> PathBuf {
        self.root.join("out")
    }

    pub fn ensure(&self) -> Result<()> {
        for dir in [
            self.root.clone(),
            self.pages(),
            self.vision(),
            self.panels(),
            self.portraits(),
            self.audio(),
            self.out(),
        ] {
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("creating {}", dir.display()))?;
        }
        Ok(())
    }
}

/// Where projects live when the user has not chosen a folder.
pub fn default_projects_root(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("projects")
}

pub fn create(projects_root: &Path, name: &str, format: SourceFormat) -> Result<Project> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("Give the project a name first."));
    }
    let id = util::new_id("proj");
    let slug = util::slugify(trimmed);
    // Keep folder names unique without making them unreadable.
    let mut dir = projects_root.join(&slug);
    if dir.exists() {
        dir = projects_root.join(format!("{slug}-{}", &id[5..11]));
    }
    let paths = Paths::new(&dir);
    paths.ensure()?;

    let now = util::now_iso();
    let project = Project {
        meta: ProjectMeta {
            id,
            name: trimmed.to_string(),
            created_at: now.clone(),
            updated_at: now,
            root: dir.to_string_lossy().to_string(),
            format,
            page_count: 0,
            chapter_count: 0,
            analyzed: false,
            has_script: false,
        },
        source: Source {
            format,
            ..Default::default()
        },
        bible: StoryBible::default(),
        scripts: BTreeMap::new(),
        usage: Default::default(),
    };
    save(&project)?;
    Ok(project)
}

pub fn save(project: &Project) -> Result<()> {
    let paths = Paths::new(&project.meta.root);
    paths.ensure()?;
    let json = serde_json::to_string_pretty(project)?;
    let manifest = paths.manifest();
    let tmp = manifest.with_extension("json.tmp");
    std::fs::write(&tmp, json).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &manifest)?;
    Ok(())
}

pub fn load(root: &Path) -> Result<Project> {
    let manifest = Paths::new(root).manifest();
    let raw = std::fs::read_to_string(&manifest)
        .with_context(|| format!("reading {}", manifest.display()))?;
    let mut project: Project = serde_json::from_str(&raw)
        .with_context(|| format!("parsing {}", manifest.display()))?;
    // The folder may have been moved; trust the path we were opened from.
    project.meta.root = root.to_string_lossy().to_string();
    Ok(project)
}

pub fn list(projects_root: &Path) -> Vec<ProjectMeta> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(projects_root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !Paths::new(&path).manifest().exists() {
            continue;
        }
        match load(&path) {
            Ok(project) => out.push(project.meta),
            Err(err) => tracing::warn!("skipping {}: {err}", path.display()),
        }
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out
}

pub fn delete(root: &Path) -> Result<()> {
    // Refuse to delete anything that is not actually a project folder.
    if !Paths::new(root).manifest().exists() {
        return Err(anyhow!(
            "{} does not look like a Recap Studio project, refusing to delete it.",
            root.display()
        ));
    }
    std::fs::remove_dir_all(root).with_context(|| format!("removing {}", root.display()))?;
    Ok(())
}

/// Refresh the denormalized counters the library view reads.
pub fn touch(project: &mut Project) {
    project.meta.updated_at = util::now_iso();
    project.meta.page_count = project.source.pages.len();
    project.meta.chapter_count = project.source.chapters.len();
    project.meta.analyzed = !project.bible.readings.is_empty();
    project.meta.has_script = project
        .scripts
        .values()
        .any(|s: &ScriptBundle| !s.final_script.trim().is_empty());
    project.source.total_panels = project.source.pages.iter().map(|p| p.panels.len()).sum();
}
