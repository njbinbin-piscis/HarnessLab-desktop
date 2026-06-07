// Minimal inline icon set (lucide-style strokes) — no extra dependency.
type P = { className?: string };
const base = {
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  viewBox: "0 0 24 24",
};

export const Icon = {
  projects: (p: P) => (
    <svg {...base} {...p}><path d="M3 7h18M3 12h18M3 17h18" /></svg>
  ),
  design: (p: P) => (
    <svg {...base} {...p}><path d="M12 3v18M3 12h18" /><circle cx="12" cy="12" r="9" /></svg>
  ),
  library: (p: P) => (
    <svg {...base} {...p}><path d="M4 4h6v16H4zM14 4h6v16h-6z" /></svg>
  ),
  experiments: (p: P) => (
    <svg {...base} {...p}><path d="M9 3v6l-5 9a2 2 0 0 0 2 3h12a2 2 0 0 0 2-3l-5-9V3" /><path d="M8 3h8" /></svg>
  ),
  factory: (p: P) => (
    <svg {...base} {...p}><path d="M3 21V9l6 4V9l6 4V5l6 4v12z" /></svg>
  ),
  market: (p: P) => (
    <svg {...base} {...p}><path d="M3 9l1-5h16l1 5M5 9v11h14V9M9 13h6" /></svg>
  ),
};
