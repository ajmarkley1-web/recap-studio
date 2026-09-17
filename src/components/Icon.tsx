// Inline SVG icons. Bundled rather than pulled from a package so the app has
// no icon-font dependency and works offline.

export type IconName =
  | "library"
  | "source"
  | "analyze"
  | "bible"
  | "script"
  | "storyboard"
  | "render"
  | "settings"
  | "plus"
  | "refresh"
  | "check"
  | "alert"
  | "x"
  | "chevron"
  | "folder"
  | "file"
  | "play"
  | "download"
  | "trash"
  | "sparkle"
  | "user"
  | "link"
  | "clock"
  | "wand"
  | "back"
  | "external"
  | "sun"
  | "moon"
  | "wave";

const PATHS: Record<IconName, JSX.Element> = {
  library: (
    <>
      <rect x="3" y="4" width="6" height="16" rx="1.5" />
      <rect x="11" y="4" width="4" height="16" rx="1.5" />
      <path d="M17.5 4.8l3 14.2" />
    </>
  ),
  source: (
    <>
      <path d="M4 5.5A1.5 1.5 0 015.5 4H10l2 2h6.5A1.5 1.5 0 0120 7.5v11A1.5 1.5 0 0118.5 20h-13A1.5 1.5 0 014 18.5z" />
      <path d="M12 11v5M9.5 13.5L12 11l2.5 2.5" />
    </>
  ),
  analyze: (
    <>
      <circle cx="11" cy="11" r="6.5" />
      <path d="M16 16l4.5 4.5M11 8v6M8 11h6" />
    </>
  ),
  bible: (
    <>
      <path d="M4 5.5A2.5 2.5 0 016.5 3H19v15H6.5A2.5 2.5 0 004 20.5z" />
      <path d="M4 18.5A2.5 2.5 0 016.5 16H19v5H6.5" />
      <path d="M8 7.5h7M8 10.5h5" />
    </>
  ),
  script: (
    <>
      <path d="M6 3h8l4 4v14H6z" />
      <path d="M14 3v4h4" />
      <path d="M9 12h6M9 15.5h6M9 8.5h2" />
    </>
  ),
  storyboard: (
    <>
      <rect x="3" y="4.5" width="7.5" height="6.5" rx="1" />
      <rect x="13.5" y="4.5" width="7.5" height="6.5" rx="1" />
      <rect x="3" y="13" width="7.5" height="6.5" rx="1" />
      <rect x="13.5" y="13" width="7.5" height="6.5" rx="1" />
    </>
  ),
  render: (
    <>
      <rect x="2.5" y="5" width="19" height="14" rx="2.5" />
      <path d="M10 9.5l5 2.5-5 2.5z" />
    </>
  ),
  settings: (
    <>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 14.5a1.6 1.6 0 00.33 1.77l.06.06a2 2 0 11-2.83 2.83l-.06-.06a1.6 1.6 0 00-1.77-.33 1.6 1.6 0 00-1 1.47V21a2 2 0 11-4 0v-.1a1.6 1.6 0 00-1.05-1.47 1.6 1.6 0 00-1.77.33l-.06.06a2 2 0 11-2.83-2.83l.06-.06a1.6 1.6 0 00.33-1.77 1.6 1.6 0 00-1.47-1H3a2 2 0 110-4h.1a1.6 1.6 0 001.47-1.05 1.6 1.6 0 00-.33-1.77l-.06-.06a2 2 0 112.83-2.83l.06.06a1.6 1.6 0 001.77.33H9a1.6 1.6 0 001-1.47V3a2 2 0 114 0v.1a1.6 1.6 0 001 1.47 1.6 1.6 0 001.77-.33l.06-.06a2 2 0 112.83 2.83l-.06.06a1.6 1.6 0 00-.33 1.77V9a1.6 1.6 0 001.47 1H21a2 2 0 110 4h-.1a1.6 1.6 0 00-1.47 1z" />
    </>
  ),
  plus: <path d="M12 5v14M5 12h14" />,
  refresh: (
    <>
      <path d="M20 11a8 8 0 10-2.3 6.4" />
      <path d="M20 5v6h-6" />
    </>
  ),
  check: <path d="M4.5 12.5l5 5 10-11" />,
  alert: (
    <>
      <path d="M12 3.5l9.5 16.5H2.5z" />
      <path d="M12 9.5v4.5M12 17.2v.1" />
    </>
  ),
  x: <path d="M5.5 5.5l13 13M18.5 5.5l-13 13" />,
  chevron: <path d="M9 5.5l7 6.5-7 6.5" />,
  folder: (
    <path d="M3 6.5A1.5 1.5 0 014.5 5H9l2 2.5h8.5A1.5 1.5 0 0121 9v9.5a1.5 1.5 0 01-1.5 1.5h-15A1.5 1.5 0 013 18.5z" />
  ),
  file: (
    <>
      <path d="M6 3h8l4 4v14H6z" />
      <path d="M14 3v4h4" />
    </>
  ),
  play: <path d="M7 4.5l12 7.5-12 7.5z" />,
  download: <path d="M12 3.5v11M7.5 10L12 14.5 16.5 10M4 19.5h16" />,
  trash: (
    <>
      <path d="M4 6.5h16M9 6.5V4h6v2.5M6.5 6.5l1 13.5h9l1-13.5" />
    </>
  ),
  sparkle: (
    <>
      <path d="M12 3l1.9 5.4L19 10l-5.1 1.6L12 17l-1.9-5.4L5 10l5.1-1.6z" />
      <path d="M18.5 15.5l.7 2 2 .7-2 .7-.7 2-.7-2-2-.7 2-.7z" />
    </>
  ),
  user: (
    <>
      <circle cx="12" cy="8.5" r="4" />
      <path d="M4.5 20.5a7.5 7.5 0 0115 0" />
    </>
  ),
  link: (
    <>
      <path d="M10 13.5a4 4 0 005.7 0l2.8-2.8a4 4 0 10-5.7-5.7l-1.4 1.4" />
      <path d="M14 10.5a4 4 0 00-5.7 0l-2.8 2.8a4 4 0 105.7 5.7l1.4-1.4" />
    </>
  ),
  clock: (
    <>
      <circle cx="12" cy="12" r="8.5" />
      <path d="M12 7v5.2l3.2 2" />
    </>
  ),
  wand: (
    <>
      <path d="M4 20l11-11" />
      <path d="M15.5 3.5l1 2.5 2.5 1-2.5 1-1 2.5-1-2.5-2.5-1 2.5-1z" />
      <path d="M6 4l.7 1.8L8.5 6.5 6.7 7.2 6 9l-.7-1.8L3.5 6.5l1.8-.7z" />
    </>
  ),
  back: <path d="M15 5.5l-7 6.5 7 6.5" />,
  external: (
    <>
      <path d="M14 4.5h5.5V10" />
      <path d="M19.5 4.5L11 13" />
      <path d="M18 14v5.5H4.5V6H10" />
    </>
  ),
  sun: (
    <>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2.5M12 19.5V22M2 12h2.5M19.5 12H22M4.9 4.9l1.8 1.8M17.3 17.3l1.8 1.8M19.1 4.9l-1.8 1.8M6.7 17.3l-1.8 1.8" />
    </>
  ),
  moon: <path d="M20 14.5A8.5 8.5 0 019.5 4a8.5 8.5 0 1010.5 10.5z" />,
  wave: (
    <>
      <path d="M4 12v2M8 8v8M12 5v14M16 8.5v7M20 11v2" />
    </>
  ),
};

interface Props {
  name: IconName;
  size?: number;
  className?: string;
  strokeWidth?: number;
  filled?: boolean;
}

export function Icon({
  name,
  size = 17,
  className,
  strokeWidth = 1.7,
  filled = false,
}: Props) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill={filled ? "currentColor" : "none"}
      stroke={filled ? "none" : "currentColor"}
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
      focusable="false"
    >
      {PATHS[name]}
    </svg>
  );
}
