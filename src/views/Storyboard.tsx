import { useState } from "react";
import { fileUrl } from "../lib/api";
import { useStore } from "../lib/store";
import { scopeKey } from "../lib/types";
import { Icon } from "../components/Icon";
import { Empty, Lightbox } from "../components/ui";

export function StoryboardView() {
  const project = useStore((s) => s.project);
  const scope = useStore((s) => s.scope);
  const setView = useStore((s) => s.setView);
  const [zoom, setZoom] = useState<string | null>(null);

  if (!project) return <Empty icon="storyboard" title="Open a project first" />;

  const bundle = project.scripts[scopeKey(scope)] ?? null;
  if (!bundle || bundle.storyboard.length === 0) {
    return (
      <Empty
        icon="storyboard"
        title="No storyboard yet"
        action={
          <button className="btn primary" onClick={() => setView("script")}>
            Go to Script studio
          </button>
        }
      >
        The storyboard is built when the narration is generated. Every panel
        becomes one shot, in reading order.
      </Empty>
    );
  }

  const narratedCount = bundle.storyboard.filter((s) => s.audio_path).length;
  const totalDuration = bundle.storyboard.reduce(
    (n, s) => n + (s.duration_secs ?? 0),
    0
  );

  return (
    <div className="page wide">
      <div className="page-head row wrap">
        <div style={{ flex: 1, minWidth: 220 }}>
          <h1>Storyboard</h1>
          <p>
            One shot per panel, in reading order, each holding the narration
            written for it. This is the shot list the video renderer follows.
          </p>
        </div>
        <span className="pill">{bundle.storyboard.length} shots</span>
        {narratedCount > 0 && (
          <span className="pill good">{narratedCount} narrated</span>
        )}
        {totalDuration > 0 && (
          <span className="pill">{Math.round(totalDuration)}s total</span>
        )}
        <button className="btn" onClick={() => setView("render")}>
          Narrate and render
          <Icon name="chevron" size={14} />
        </button>
      </div>

      <div>
        {bundle.storyboard.map((shot) => (
          <div key={shot.index} className="shot">
            <div className="shot-num">
              {String(shot.index + 1).padStart(2, "0")}
              {shot.duration_secs != null && (
                <div className="faint">{shot.duration_secs.toFixed(1)}s</div>
              )}
              {shot.audio_path && (
                <div style={{ color: "var(--good)", marginTop: 4 }}>
                  <Icon name="wave" size={13} />
                </div>
              )}
            </div>

            <div>
              <div className="shot-text">{shot.text}</div>
              {shot.audio_path && (
                <audio
                  controls
                  preload="none"
                  src={fileUrl(shot.audio_path)}
                  style={{ width: "100%", maxWidth: 320, marginTop: 8, height: 32 }}
                />
              )}
            </div>

            <div className="shot-thumbs">
              {shot.image_paths.length === 0 ? (
                <span className="faint small">panel image missing</span>
              ) : (
                shot.image_paths.map((path, i) => (
                  <img
                    key={i}
                    src={fileUrl(path)}
                    alt=""
                    loading="lazy"
                    onClick={() => setZoom(fileUrl(path))}
                  />
                ))
              )}
            </div>
          </div>
        ))}
      </div>

      <Lightbox src={zoom} onClose={() => setZoom(null)} />
    </div>
  );
}
