//! Video assembly through ffmpeg.
//!
//! One segment is rendered per storyboard shot, so each shot's panels stay on
//! screen for exactly as long as its narration. The segments are then stream
//! copied into the final file, which is fast and lossless.
//!
//! ffmpeg is not bundled. `locate` looks in the usual places and the app guides
//! the user through installing it if it is missing.

use crate::events::Emitter;
use crate::model::{Project, ScriptBundle};
use crate::project::Paths;
use crate::settings::{Settings, VideoSettings};
use anyhow::{anyhow, Context, Result};
use image::imageops::FilterType;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct FfmpegStatus {
    pub available: bool,
    pub ffmpeg_path: Option<String>,
    pub ffprobe_path: Option<String>,
    pub version: Option<String>,
    pub install_hint: String,
}

fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(target_os = "windows")]
    {
        for base in [
            r"C:\ffmpeg\bin",
            r"C:\Program Files\ffmpeg\bin",
            r"C:\ProgramData\chocolatey\bin",
        ] {
            dirs.push(PathBuf::from(base));
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            dirs.push(PathBuf::from(&local).join("Microsoft\\WinGet\\Links"));
            dirs.push(PathBuf::from(&local).join("ffmpeg\\bin"));
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        for base in ["/usr/bin", "/usr/local/bin", "/opt/homebrew/bin", "/snap/bin"] {
            dirs.push(PathBuf::from(base));
        }
    }
    dirs
}

fn exe_name(stem: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

fn find_tool(stem: &str, override_path: &str) -> Option<PathBuf> {
    // An explicit setting wins. It may point at the binary or at its folder.
    let trimmed = override_path.trim();
    if !trimmed.is_empty() {
        let p = PathBuf::from(trimmed);
        if p.is_file() {
            if p.file_stem()
                .map(|s| s.to_string_lossy().eq_ignore_ascii_case(stem))
                .unwrap_or(false)
            {
                return Some(p);
            }
            // Pointed at ffmpeg but we want ffprobe: look next door.
            if let Some(dir) = p.parent() {
                let sibling = dir.join(exe_name(stem));
                if sibling.is_file() {
                    return Some(sibling);
                }
            }
        }
        if p.is_dir() {
            let candidate = p.join(exe_name(stem));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    if let Ok(found) = which::which(stem) {
        return Some(found);
    }
    for dir in candidate_dirs() {
        let candidate = dir.join(exe_name(stem));
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

pub fn locate(settings: &VideoSettings) -> FfmpegStatus {
    let ffmpeg = find_tool("ffmpeg", &settings.ffmpeg_path);
    let ffprobe = find_tool("ffprobe", &settings.ffmpeg_path);
    let version = ffmpeg.as_ref().and_then(|p| {
        Command::new(p)
            .arg("-version")
            .output()
            .ok()
            .and_then(|out| String::from_utf8(out.stdout).ok())
            .and_then(|s| s.lines().next().map(str::to_string))
    });

    let install_hint = if cfg!(target_os = "windows") {
        "Install with `winget install Gyan.FFmpeg` in a terminal, then reopen Recap Studio. \
         Or download a build from ffmpeg.org, unzip it, and point the ffmpeg path in Settings at its bin folder."
            .to_string()
    } else if cfg!(target_os = "macos") {
        "Install with `brew install ffmpeg`, then reopen Recap Studio.".to_string()
    } else {
        "Install with `sudo apt install ffmpeg` (or your distribution's equivalent), then reopen Recap Studio."
            .to_string()
    };

    FfmpegStatus {
        available: ffmpeg.is_some() && ffprobe.is_some(),
        ffmpeg_path: ffmpeg.map(|p| p.to_string_lossy().to_string()),
        ffprobe_path: ffprobe.map(|p| p.to_string_lossy().to_string()),
        version,
        install_hint,
    }
}

fn run(tool: &Path, args: &[String], what: &str) -> Result<String> {
    let output = Command::new(tool)
        .args(args)
        .output()
        .with_context(|| format!("running {}", tool.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // ffmpeg's real error is almost always in the last few lines.
        let tail: Vec<&str> = stderr.lines().rev().take(8).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        return Err(anyhow!("{what} failed:\n{}", tail.join("\n")));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn probe_duration(ffprobe: &Path, media: &Path) -> Result<f32> {
    let args: Vec<String> = vec![
        "-v".into(),
        "error".into(),
        "-show_entries".into(),
        "format=duration".into(),
        "-of".into(),
        "default=noprint_wrappers=1:nokey=1".into(),
        media.to_string_lossy().to_string(),
    ];
    let out = run(ffprobe, &args, "Reading audio duration")?;
    out.trim()
        .parse::<f32>()
        .map_err(|_| anyhow!("could not read a duration from {}", media.display()))
}

/// Fit an image into the target frame, filling the sides with a blurred copy of
/// itself so tall webtoon panels do not sit on flat black bars.
fn letterbox(src: &Path, dest: &Path, cfg: &VideoSettings) -> Result<()> {
    let img = image::ImageReader::open(src)
        .with_context(|| format!("opening {}", src.display()))?
        .with_guessed_format()?
        .decode()?;
    let (w, h) = (cfg.width, cfg.height);

    let mut canvas = if cfg.blurred_background {
        // Cover the frame, blur it, then darken so the foreground still reads.
        let cover = img.resize_to_fill(w, h, FilterType::Triangle);
        let mut blurred = image::imageops::blur(&cover.to_rgb8(), 22.0);
        for pixel in blurred.pixels_mut() {
            pixel.0[0] = (pixel.0[0] as f32 * 0.45) as u8;
            pixel.0[1] = (pixel.0[1] as f32 * 0.45) as u8;
            pixel.0[2] = (pixel.0[2] as f32 * 0.45) as u8;
        }
        blurred
    } else {
        image::RgbImage::from_pixel(w, h, image::Rgb([8, 8, 10]))
    };

    let fitted = img.resize(w, h, FilterType::Lanczos3).to_rgb8();
    let x = ((w.saturating_sub(fitted.width())) / 2) as i64;
    let y = ((h.saturating_sub(fitted.height())) / 2) as i64;
    image::imageops::overlay(&mut canvas, &fitted, x, y);

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    canvas
        .save(dest)
        .with_context(|| format!("writing frame {}", dest.display()))?;
    Ok(())
}

/// ffmpeg's concat demuxer wants forward slashes and escaped quotes, on every OS.
fn concat_line(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/").replace('\'', "'\\''");
    format!("file '{normalized}'")
}

pub struct RenderOutcome {
    pub path: PathBuf,
    pub duration_secs: f32,
    pub shots: usize,
}

pub fn render(
    project: &Project,
    bundle: &mut ScriptBundle,
    settings: &Settings,
    emitter: &Emitter,
) -> Result<RenderOutcome> {
    let status = locate(&settings.video);
    let (Some(ffmpeg), Some(ffprobe)) = (status.ffmpeg_path.clone(), status.ffprobe_path.clone())
    else {
        return Err(anyhow!(
            "ffmpeg was not found on this machine. {}",
            status.install_hint
        ));
    };
    let ffmpeg = PathBuf::from(ffmpeg);
    let ffprobe = PathBuf::from(ffprobe);

    let narrated: Vec<usize> = bundle
        .storyboard
        .iter()
        .enumerate()
        .filter(|(_, s)| s.audio_path.as_ref().map(|p| Path::new(p).exists()).unwrap_or(false))
        .map(|(i, _)| i)
        .collect();
    if narrated.is_empty() {
        return Err(anyhow!(
            "No narration audio found. Run Narrate before rendering the video."
        ));
    }

    let paths = Paths::new(&project.meta.root);
    let work = paths.out().join("render");
    // Start from a clean slate so a previous failed render cannot leak frames in.
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(work.join("frames"))?;
    std::fs::create_dir_all(work.join("segments"))?;

    let cfg = &settings.video;
    let total = narrated.len();
    let mut segment_files: Vec<PathBuf> = Vec::new();
    let mut total_duration = 0.0f32;

    for (position, shot_index) in narrated.iter().enumerate() {
        let shot = &bundle.storyboard[*shot_index];
        let audio = PathBuf::from(shot.audio_path.as_ref().expect("filtered above"));
        let duration = probe_duration(&ffprobe, &audio)?.max(0.4);

        // Every shot needs at least one image.
        let mut images: Vec<PathBuf> = shot
            .image_paths
            .iter()
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .collect();
        if images.is_empty() {
            if let Some(page) = project.source.pages.first() {
                images.push(PathBuf::from(&page.path));
            } else {
                return Err(anyhow!("Shot {} has no image to show.", shot_index + 1));
            }
        }

        let per_image = duration / images.len() as f32;
        let mut list = String::new();
        let mut last_frame = None;
        for (j, src) in images.iter().enumerate() {
            let frame = work
                .join("frames")
                .join(format!("s{position:04}-{j:02}.png"));
            letterbox(src, &frame, cfg)?;
            list.push_str(&concat_line(&frame));
            list.push('\n');
            list.push_str(&format!("duration {per_image:.4}\n"));
            last_frame = Some(frame);
        }
        // The concat demuxer ignores the last entry's duration unless the file
        // is listed once more, so repeat it.
        if let Some(frame) = last_frame {
            list.push_str(&concat_line(&frame));
            list.push('\n');
        }
        let list_path = work.join(format!("shot-{position:04}.txt"));
        std::fs::write(&list_path, list)?;

        let segment = work.join("segments").join(format!("shot-{position:04}.mp4"));
        let mut filters = vec![format!(
            "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:-1:-1:color=black,setsar=1",
            cfg.width, cfg.height, cfg.width, cfg.height
        )];
        if cfg.ken_burns {
            // A gentle push in across the shot. `on` is the output frame index,
            // so the zoom advances smoothly and tops out just under 1.1x.
            filters.push(format!(
                "zoompan=z='min(1.0+0.0009*on,1.09)':d=1:x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)':s={}x{}:fps={}",
                cfg.width, cfg.height, cfg.fps
            ));
        }
        if cfg.fade_secs > 0.01 && duration > cfg.fade_secs * 2.5 {
            let out_start = duration - cfg.fade_secs;
            filters.push(format!("fade=t=in:st=0:d={:.3}", cfg.fade_secs));
            filters.push(format!(
                "fade=t=out:st={out_start:.3}:d={:.3}",
                cfg.fade_secs
            ));
        }

        let args: Vec<String> = vec![
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-y".into(),
            "-f".into(),
            "concat".into(),
            "-safe".into(),
            "0".into(),
            "-i".into(),
            list_path.to_string_lossy().to_string(),
            "-i".into(),
            audio.to_string_lossy().to_string(),
            "-vf".into(),
            filters.join(","),
            "-r".into(),
            cfg.fps.to_string(),
            "-c:v".into(),
            "libx264".into(),
            "-preset".into(),
            "medium".into(),
            "-crf".into(),
            cfg.crf.to_string(),
            "-pix_fmt".into(),
            "yuv420p".into(),
            "-c:a".into(),
            "aac".into(),
            "-b:a".into(),
            "192k".into(),
            "-ar".into(),
            "44100".into(),
            "-ac".into(),
            "2".into(),
            "-t".into(),
            format!("{duration:.4}"),
            "-movflags".into(),
            "+faststart".into(),
            segment.to_string_lossy().to_string(),
        ];
        run(&ffmpeg, &args, &format!("Rendering shot {}", position + 1))?;

        bundle.storyboard[*shot_index].duration_secs = Some(duration);
        total_duration += duration;
        segment_files.push(segment);

        emitter.progress(
            "render",
            position + 1,
            total,
            format!("Rendered shot {}/{total}", position + 1),
        );
    }

    // Join the segments. They share codec settings, so a stream copy is safe.
    emitter.stage("render", "Joining segments");
    let mut joined = String::new();
    for segment in &segment_files {
        joined.push_str(&concat_line(segment));
        joined.push('\n');
    }
    let join_list = work.join("segments.txt");
    std::fs::write(&join_list, joined)?;

    let final_path = paths.out().join("recap.mp4");
    let args: Vec<String> = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-i".into(),
        join_list.to_string_lossy().to_string(),
        "-c".into(),
        "copy".into(),
        "-movflags".into(),
        "+faststart".into(),
        final_path.to_string_lossy().to_string(),
    ];
    run(&ffmpeg, &args, "Joining the final video")?;

    // Frames are large and only needed during the render.
    let _ = std::fs::remove_dir_all(work.join("frames"));

    emitter.done(format!(
        "Video ready: {} shots, {:.0}s",
        total, total_duration
    ));
    Ok(RenderOutcome {
        path: final_path,
        duration_secs: total_duration,
        shots: total,
    })
}
