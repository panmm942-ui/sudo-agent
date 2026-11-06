/**
 * In-place shape morph between island pages. Home's logo, the album art and the Sudo orb all sit
 * in one box (the album art's box, measured on the Spotify page), so a page change reshapes that
 * box where it stands instead of flying anything around:
 *   logo (capsule outline)  ↔  art (rounded square, the cover revealed through it)  ↔  orb (circle).
 * The cover stays still while its window reshapes, which reads as one object changing form.
 */
export type MorphShape = "art" | "orb" | "logo";
/** The shared box, in screen px, plus the album art's corner radius. */
export type MorphBox = { left: number; top: number; width: number; height: number; radius: number };

type Geometry = { x: number; y: number; w: number; h: number; r: number };

/** The capsule logo inside its box (matches SudoLogo's 92-unit viewBox: rect 10,31 72×30). */
const LOGO = { x: 10 / 92, y: 31 / 92, w: 72 / 92, h: 30 / 92, stroke: 4.2 / 92 };

function geometry(shape: MorphShape, box: MorphBox): Geometry {
  const { width: w, height: h } = box;
  if (shape === "logo") {
    const lh = h * LOGO.h;
    return { x: w * LOGO.x, y: h * LOGO.y, w: w * LOGO.w, h: lh, r: lh / 2 };
  }
  if (shape === "orb") return { x: 0, y: 0, w, h, r: Math.min(w, h) / 2 };
  return { x: 0, y: 0, w, h, r: box.radius };
}

let active: { finish: () => void } | null = null;

/** Ends a running morph at once (a new page change arrived). */
export function finishMorph(): void {
  active?.finish();
}

/**
 * Reshapes `from` into `to` inside `box`. `hide` are the real elements under the box (the old and
 * new page's logo / art); they stay invisible until the morph hands back to them.
 */
export type MorphHooks = {
  /** Linear progress 0..1, every frame while the morph runs. */
  onprogress?: (t: number) => void;
  /** The morph ended (finished or cut short by a newer page change). */
  onend?: () => void;
};

export function morph(from: MorphShape, to: MorphShape, box: MorphBox, artwork: string | null, hide: () => HTMLElement[], duration = 520, hooks: MorphHooks = {}): Promise<void> {
  finishMorph();
  const a = geometry(from, box), b = geometry(to, box);
  const ring = Math.max(1.5, box.width * LOGO.stroke);

  const overlay = document.createElement("div");
  overlay.style.cssText = `position:fixed;left:${box.left}px;top:${box.top}px;width:${box.width}px;height:${box.height}px;` +
    "z-index:50;pointer-events:none;contain:layout paint;";
  const shape = document.createElement("div");
  shape.style.cssText = "position:absolute;overflow:hidden;will-change:left,top,width,height,border-radius;";
  // The cover is laid out at the full box and counter-moved, so it never scales or slides.
  const cover = document.createElement("div");
  cover.style.cssText = `position:absolute;width:${box.width}px;height:${box.height}px;will-change:transform,opacity;`;
  if (artwork) cover.style.background = `center / cover no-repeat url("${artwork.replace(/"/g, "%22")}")`;
  const outline = document.createElement("div");
  outline.style.cssText = `position:absolute;inset:0;border-radius:inherit;box-shadow:inset 0 0 0 ${ring}px #d9dde4;will-change:opacity;`;
  shape.append(cover, outline);
  overlay.append(shape);
  document.body.append(overlay);

  const hidden = new Set<HTMLElement>();
  const conceal = () => { for (const el of hide()) if (!hidden.has(el)) { hidden.add(el); el.style.opacity = "0"; } };
  conceal();
  // The new page mounts a frame later; hide its element as soon as it exists.
  const raf = requestAnimationFrame(conceal);

  // Quick to leave, long soft landing: reads as one object settling into its new form.
  const ease = "cubic-bezier(.32,.72,0,1)";
  const frame = (g: Geometry) => ({ left: `${g.x}px`, top: `${g.y}px`, width: `${g.w}px`, height: `${g.h}px`, borderRadius: `${g.r}px` });
  const shift = (g: Geometry) => ({ transform: `translate(${-g.x}px,${-g.y}px)` });
  const timing = { duration, easing: ease, fill: "forwards" as const };
  const coverOn = (s: MorphShape) => (s === "art" ? 1 : 0);
  const outlineOn = (s: MorphShape) => (s === "logo" ? 1 : 0);
  const animations = [
    shape.animate([frame(a), frame(b)], timing),
    // Cover ↔ orb: the cover melts (blur, a little smaller) while the live orb grows out of it in
    // the same place (the island drives the orb from `onprogress`), so nothing slides or pops.
    from === "orb" || to === "orb"
      ? cover.animate(
          from === "orb"
            ? [{ ...shift(a), opacity: 0, filter: "blur(10px)", scale: "0.9" }, { opacity: 0, filter: "blur(10px)", scale: "0.9", offset: 0.25 }, { ...shift(b), opacity: coverOn(to), filter: "blur(0px)", scale: "1" }]
            : [{ ...shift(a), opacity: coverOn(from), filter: "blur(0px)", scale: "1" }, { opacity: 0, filter: "blur(10px)", scale: "0.9", offset: 0.7 }, { ...shift(b), opacity: 0, filter: "blur(10px)", scale: "0.9" }],
          timing,
        )
      : cover.animate([{ ...shift(a), opacity: coverOn(from) }, { ...shift(b), opacity: coverOn(to) }], timing),
    // The outline leaves quickly (or arrives late) so the cover and the ring never muddy each other.
    outline.animate(
      from === "logo"
        ? [{ opacity: 1 }, { opacity: 0, offset: 0.45 }, { opacity: outlineOn(to) }]
        : [{ opacity: 0 }, { opacity: 0, offset: 0.4 }, { opacity: outlineOn(to) }],
      { duration, easing: "ease-out", fill: "forwards" },
    ),
  ];

  let done = false;
  let resolve!: () => void;
  const finished = new Promise<void>((r) => (resolve = r));
  const started = performance.now();
  let tick = 0;
  const progress = (now: number) => {
    hooks.onprogress?.(Math.min(1, (now - started) / duration));
    if (!done) tick = requestAnimationFrame(progress);
  };
  if (hooks.onprogress) { hooks.onprogress(0); tick = requestAnimationFrame(progress); }
  const finish = () => {
    if (done) return;
    done = true;
    cancelAnimationFrame(raf);
    cancelAnimationFrame(tick);
    hooks.onprogress?.(1);
    hooks.onend?.();
    // The overlay ends exactly on the real element, so hand over in the same frame.
    for (const el of hidden) el.style.opacity = "";
    for (const animation of animations) animation.cancel();
    overlay.remove();
    if (active?.finish === finish) active = null;
    resolve();
  };
  active = { finish };
  void animations[0].finished.then(finish, finish);
  return finished;
}

const ORB_FACE =
  "radial-gradient(circle at 38% 34%, #fffdf6 0 16%, oklch(.84 .1 85) 44%, oklch(.66 .14 150) 82%, oklch(.5 .12 200) 100%)";

export type Rect = { left: number; top: number; width: number; height: number; radius: number };

/** CSS cubic-bezier(x1,y1,x2,y2) as a function of progress 0..1. */
export function cubicBezier(x1: number, y1: number, x2: number, y2: number): (t: number) => number {
  const at = (a: number, b: number, t: number) => 3 * a * t * (1 - t) ** 2 + 3 * b * t * t * (1 - t) + t ** 3;
  return (t) => {
    if (t <= 0) return 0;
    if (t >= 1) return 1;
    let lo = 0, hi = 1, u = t;
    for (let i = 0; i < 24; i++) { u = (lo + hi) / 2; if (at(x1, x2, u) < t) lo = u; else hi = u; }
    return at(y1, y2, u);
  };
}
const landing = cubicBezier(0.2, 0.8, 0.2, 1);
const mix = (a: number, b: number, k: number) => a + (b - a) * k;

/**
 * Collapse, timed with the island's own shrink (fast start, soft landing):
 * - the song cover flies into the small island's cover thumbnail and lands exactly on it;
 * - the capsule logo becomes the small island's outline, then dissolves into its glass;
 * - the orb drops into the small island's orb (or into the pill, dissolving, when it has none).
 * `source` is the real element (hidden meanwhile). `target` is read every frame: the island is
 * still springing (and may be pushed down by edge avoidance or burn-in drift), so the piece
 * follows where its landing spot is now instead of flying to a guess and hanging there.
 */
export function toCompact(shape: MorphShape, source: HTMLElement, artwork: string | null, radius: number,
  target: () => Rect, dissolve: boolean, duration = 340): Promise<void> {
  finishMorph();
  const box = source.getBoundingClientRect();
  if (box.width < 4) return Promise.resolve();
  const g = geometry(shape, { left: box.left, top: box.top, width: box.width, height: box.height, radius });
  const ring = Math.max(1.5, box.width * LOGO.stroke);
  const from = { left: box.left + g.x, top: box.top + g.y, width: g.w, height: g.h, radius: g.r };
  const el = document.createElement("div");
  el.style.cssText = `position:fixed;left:0;top:0;width:${from.width}px;height:${from.height}px;border-radius:${from.radius}px;` +
    `transform:translate(${from.left}px,${from.top}px);z-index:50;pointer-events:none;overflow:hidden;will-change:transform,width,height,opacity;`;
  if (shape === "art" && artwork) el.style.background = `center / cover no-repeat url("${artwork.replace(/"/g, "%22")}")`;
  else if (shape === "orb") el.style.background = ORB_FACE;
  else el.style.boxShadow = `inset 0 0 0 ${ring}px #d9dde4`;
  document.body.append(el);
  source.style.opacity = "0";

  let done = false;
  let resolve!: () => void;
  const finished = new Promise<void>((r) => (resolve = r));
  const start = performance.now();
  let raf = 0;
  const paint = (now: number) => {
    const t = Math.min(1, (now - start) / duration);
    const k = landing(t);
    const to = target();
    const w = mix(from.width, to.width, k), h = mix(from.height, to.height, k);
    el.style.transform = `translate(${mix(from.left, to.left, k)}px,${mix(from.top, to.top, k)}px)`;
    el.style.width = `${w}px`;
    el.style.height = `${h}px`;
    el.style.borderRadius = `${mix(from.radius, to.radius, k)}px`;
    // Dissolving pieces hold until ~78% of the way, then fade into the glass.
    el.style.opacity = dissolve ? String(t < 0.78 ? 1 - 0.1 * (t / 0.78) : 0.9 * (1 - (t - 0.78) / 0.22)) : "1";
    if (shape === "logo") el.style.boxShadow = `inset 0 0 0 ${mix(ring, 1, k)}px rgba(217,221,228,${1 - k})`;
    if (t >= 1) { finish(); return; }
    raf = requestAnimationFrame(paint);
  };
  const finish = () => {
    if (done) return;
    done = true;
    cancelAnimationFrame(raf);
    el.remove();
    source.style.opacity = "";
    if (active?.finish === finish) active = null;
    resolve();
  };
  active = { finish };
  raf = requestAnimationFrame(paint);
  return finished;
}
