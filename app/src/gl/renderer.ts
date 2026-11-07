/** WebGL2 liquid-glass renderer: one full-window quad, premultiplied alpha, DPR-correct. */

import vertSrc from "./quad.vert?raw";
import glassSrc from "./glass.frag?raw";
import { TINT_ALPHA_DARK, bitmapLuminance, meanLuminance } from "./tint";
import {borderIndex,type IslandBorder} from './borders';

/** Distance from the window top to the pill's top edge, logical px. */
export const PILL_TOP = 10;

/** Pill geometry in window-relative logical px (top-left origin). */
export type PillRect = { x: number; y: number; w: number; h: number; r: number };

/** Raw pixel frame; `bgra` marks Windows.Graphics.Capture byte order. */
export type RawFrame = { width: number; height: number; data: Uint8Array; bgra?: boolean };
export type BackdropSource = ImageBitmap | RawFrame | null;

/** Per-frame values handed to extra passes (e.g. the orb). Lengths in logical px. */
export type FrameInfo = { timeMs: number; dpr: number; viewport: { w: number; h: number }; pill: PillRect };

/** An extra draw pass composited after the glass (premultiplied "over" blending is active). */
export interface RenderPass {
  /** Creates GL resources; called when added and again after a context restore. */
  init?(gl: WebGL2RenderingContext): void;
  draw(gl: WebGL2RenderingContext, frame: FrameInfo): void;
  dispose?(): void;
}

/** sRGB-encoded tint colour from the Global Constraints: oklch(0.18 0.01 260); alpha is adaptive. */
export const TINT_RGB = oklchToSrgb(0.18, 0.01, 260);

export class GlassRenderer {
  private readonly canvas: HTMLCanvasElement;
  private viewport: { w: number; h: number };
  private readonly resizeObserver: ResizeObserver;
  private gl!: WebGL2RenderingContext;
  private program!: WebGLProgram;
  private vao!: WebGLVertexArrayObject;
  private texture!: WebGLTexture;
  private u!: Record<string, WebGLUniformLocation | null>;
  private shape = { w: 126, h: 34, r: 17, y: PILL_TOP };
  private backdrop: BackdropSource = null;
  private backdropRect: [number, number, number, number] | null = null;
  private passes: RenderPass[] = [];
  /** Size of the texture's current storage (raw frames of the same size use texSubImage2D). */
  private texSize = { w: 1, h: 1 };
  private mipmapsDirty = true;
  private mipmapsEnabled = false;
  private lost = false;
  private tintAlpha = TINT_ALPHA_DARK;
  private tintRgb = TINT_RGB;
  private tintBlend = 0;
  private outline = 0;
  private darkening = 0;
  /** 0 = Sudo glass, 1 = Glisse liquid glass; eased by the caller for a smooth switch. */
  glisse = 0;
  /** Music-reactive edge glow (0..1) and its sRGB colour. */
  private pulse = 0;
  private pulseRgb: [number, number, number] = [1, 1, 1];
  private opacity = 1;
  private blur = 0;
  /** Mean luminance (0..1) of the current backdrop; null without one. */
  backdropLuminance: number | null = null;
  /** Current accent as [L, C, h]; consumed by content/orb passes. */
  accent: [number, number, number] = [0.8, 0.11, 70];

  constructor(canvas: HTMLCanvasElement, onResize?: () => void) {
    this.canvas = canvas;
    this.viewport = { w: canvas.clientWidth, h: canvas.clientHeight };
    this.resizeObserver = new ResizeObserver(entries => {
      const rect = entries[0]?.contentRect;
      if (rect && (rect.width !== this.viewport.w || rect.height !== this.viewport.h)) {
        this.viewport = { w: rect.width, h: rect.height };
        onResize?.();
      }
    });
    this.resizeObserver.observe(canvas);
    canvas.addEventListener("webglcontextlost", this.onLost);
    canvas.addEventListener("webglcontextrestored", this.onRestored);
    this.init();
  }

  /** Small horizontal displacement of the centred pill (burn-in protection). */
  offsetX = 0;
  setShape(w: number, h: number, r: number, y = PILL_TOP): void {
    this.shape = { w, h, r, y };
  }

  /** Window-relative pill rect for the current shape: centred, PILL_TOP below the top. */
  pillRect(): PillRect {
    const { w, h, r, y } = this.shape;
    return { x: (this.viewport.w - w) / 2 + this.offsetX, y, w, h, r: Math.min(r, w / 2, h / 2) };
  }

  /** Sets (or clears) the image behind the island. Null → tinted glass only (acrylic fallback). */
  setBackdrop(src: BackdropSource): void {
    this.backdrop = src;
    if (!src) this.backdropLuminance = null;
    else if ("data" in src) this.backdropLuminance = meanLuminance(src.data, src.width, src.height, src.bgra);
    else this.backdropLuminance = bitmapLuminance(src);
    if (src && !this.lost) this.upload(src);
  }

  /** Glass tint alpha (0.55 dark backdrops .. 0.72 bright ones, see tint.ts). */
  setTintAlpha(a: number): void {
    this.tintAlpha = a;
  }

  setTintColor(rgb?: [number, number, number], blend = rgb ? 1 : 0): void {
    this.tintRgb = rgb ?? TINT_RGB;
    this.tintBlend = blend;
  }

  setAppearance(outline: IslandBorder|boolean, darkening = 0, opacity = 1, blur = 0): void {
    this.outline = borderIndex(outline);
    this.darkening = darkening;
    this.opacity = opacity;
    this.blur = blur;
  }

  /** Window-relative logical rect the backdrop texture covers; null = the whole window. */
  setBackdropRect(rect: { x: number; y: number; w: number; h: number } | null): void {
    this.backdropRect = rect ? [rect.x, rect.y, rect.w, rect.h] : null;
  }

  setPulse(amount: number, rgb: [number, number, number] = [1, 1, 1]): void {
    this.pulse = Math.max(0, Math.min(1, amount));
    this.pulseRgb = rgb;
  }

  setAccent(oklch: [number, number, number]): void {
    this.accent = oklch;
  }

  /** Adds a pass drawn after the glass every frame; returns its remover. */
  addPass(pass: RenderPass): () => void {
    this.passes.push(pass);
    if (!this.lost) pass.init?.(this.gl);
    return () => {
      this.passes = this.passes.filter((p) => p !== pass);
      pass.dispose?.();
    };
  }

  render(timeMs: number): void {
    if (this.lost) return;
    const { gl, canvas } = this;
    const dpr = window.devicePixelRatio || 1;
    const cw = this.viewport.w;
    const ch = this.viewport.h;
    const pw = Math.max(1, Math.round(cw * dpr));
    const ph = Math.max(1, Math.round(ch * dpr));
    if (canvas.width !== pw || canvas.height !== ph) {
      canvas.width = pw;
      canvas.height = ph;
    }
    gl.viewport(0, 0, pw, ph);
    gl.disable(gl.SCISSOR_TEST);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);

    const pill = this.pillRect();
    const u = this.u;
    gl.useProgram(this.program);
    gl.bindVertexArray(this.vao);
    gl.uniform2f(u.u_viewport, cw, ch);
    gl.uniform1f(u.u_dpr, pw / cw);
    gl.uniform2f(u.u_center, pill.x + pill.w / 2, pill.y + pill.h / 2);
    gl.uniform2f(u.u_size, pill.w, pill.h);
    gl.uniform1f(u.u_radius, pill.r);
    gl.uniform4f(u.u_tint, ...this.tintRgb, this.tintAlpha);
    gl.uniform1f(u.u_colored, this.tintBlend);
    gl.uniform1f(u.u_outline, this.outline);
    gl.uniform1f(u.u_darkening, this.darkening);
    gl.uniform1f(u.u_opacity, this.opacity);
    gl.uniform1f(u.u_blur, this.blur / 100);
    gl.uniform1f(u.u_glisse, this.glisse);
    gl.uniform1f(u.u_pulse, this.pulse);
    gl.uniform3f(u.u_pulseColor, ...this.pulseRgb);
    gl.uniform1i(u.u_hasBackdrop, this.backdrop ? 1 : 0);
    gl.uniform1i(u.u_bgra, this.backdrop && "data" in this.backdrop && this.backdrop.bgra ? 1 : 0);
    gl.uniform4f(u.u_backdropRect, ...(this.backdropRect ?? [0, 0, cw, ch]));
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    const needsMipmaps = this.blur > 0 && this.backdrop !== null;
    if (needsMipmaps && this.mipmapsDirty) {
      gl.generateMipmap(gl.TEXTURE_2D);
      this.mipmapsDirty = false;
    }
    if (needsMipmaps !== this.mipmapsEnabled) {
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, needsMipmaps ? gl.LINEAR_MIPMAP_LINEAR : gl.LINEAR);
      this.mipmapsEnabled = needsMipmaps;
    }
    gl.uniform1i(u.u_backdrop, 0);
    // Pixels beyond the pill's shadow are transparent. Avoid shading that
    // empty window area, especially while the compact island is animating.
    const margin = 32;
    const x0 = Math.max(0, Math.floor((pill.x - margin) * dpr));
    const y0 = Math.max(0, Math.floor((ch - pill.y - pill.h - margin) * dpr));
    const x1 = Math.min(pw, Math.ceil((pill.x + pill.w + margin) * dpr));
    const y1 = Math.min(ph, Math.ceil((ch - pill.y + margin) * dpr));
    gl.enable(gl.SCISSOR_TEST);
    gl.scissor(x0, y0, Math.max(0, x1 - x0), Math.max(0, y1 - y0));
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.disable(gl.SCISSOR_TEST);

    if (this.passes.length) {
      const frame: FrameInfo = { timeMs, dpr: pw / cw, viewport: { w: cw, h: ch }, pill };
      for (const p of this.passes) p.draw(gl, frame);
    }
  }

  dispose(): void {
    this.resizeObserver.disconnect();
    this.canvas.removeEventListener("webglcontextlost", this.onLost);
    this.canvas.removeEventListener("webglcontextrestored", this.onRestored);
    for (const p of this.passes) p.dispose?.();
    this.passes = [];
    if (!this.lost) {
      this.gl.deleteTexture(this.texture);
      this.gl.deleteVertexArray(this.vao);
      this.gl.deleteProgram(this.program);
    }
  }

  private init(): void {
    const gl = this.canvas.getContext("webgl2", {
      alpha: true,
      premultipliedAlpha: true,
      antialias: false,
      depth: false,
      stencil: false,
      preserveDrawingBuffer: false,
      powerPreference: "low-power",
    });
    if (!gl) throw new Error("WebGL2 unavailable");
    this.gl = gl;
    this.program = link(gl, vertSrc, glassSrc);
    this.vao = gl.createVertexArray()!;
    this.texture = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array(4));
    this.texSize = { w: 1, h: 1 };
    this.mipmapsDirty = true;
    this.mipmapsEnabled = false;
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    const names = [
      "u_viewport", "u_dpr", "u_center", "u_size", "u_radius", "u_backdrop",
      "u_hasBackdrop", "u_bgra", "u_backdropRect", "u_tint", "u_colored", "u_outline", "u_darkening", "u_opacity", "u_blur", "u_glisse", "u_pulse", "u_pulseColor",
    ];
    this.u = Object.fromEntries(names.map((n) => [n, gl.getUniformLocation(this.program, n)]));
    if (this.backdrop) this.upload(this.backdrop);
  }

  private upload(src: Exclude<BackdropSource, null>): void {
    const gl = this.gl;
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false);
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
    if ("data" in src) {
      gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
      if (src.width === this.texSize.w && src.height === this.texSize.h) {
        gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, src.width, src.height, gl.RGBA, gl.UNSIGNED_BYTE, src.data);
      } else {
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, src.width, src.height, 0, gl.RGBA, gl.UNSIGNED_BYTE, src.data);
      }
      this.texSize = { w: src.width, h: src.height };
    } else {
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, src);
      this.texSize = { w: src.width, h: src.height };
    }
    this.mipmapsDirty = true;
  }

  private onLost = (e: Event): void => {
    e.preventDefault();
    this.lost = true;
  };

  private onRestored = (): void => {
    this.lost = false;
    this.init();
    for (const p of this.passes) p.init?.(this.gl);
  };
}

function link(gl: WebGL2RenderingContext, vs: string, fs: string): WebGLProgram {
  const compile = (type: number, src: string) => {
    const sh = gl.createShader(type)!;
    gl.shaderSource(sh, src);
    gl.compileShader(sh);
    if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) throw new Error(`shader: ${gl.getShaderInfoLog(sh)}`);
    return sh;
  };
  const prog = gl.createProgram()!;
  const v = compile(gl.VERTEX_SHADER, vs);
  const f = compile(gl.FRAGMENT_SHADER, fs);
  gl.attachShader(prog, v);
  gl.attachShader(prog, f);
  gl.linkProgram(prog);
  gl.deleteShader(v);
  gl.deleteShader(f);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) throw new Error(`link: ${gl.getProgramInfoLog(prog)}`);
  return prog;
}

/** OKLCH → gamma-encoded sRGB in [0, 1] (clipped), for compositing like CSS does. */
export function oklchToSrgb(l: number, c: number, hDeg: number): [number, number, number] {
  const h = (hDeg * Math.PI) / 180;
  const a = c * Math.cos(h);
  const b = c * Math.sin(h);
  const l_ = (l + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m_ = (l - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s_ = (l - 0.0894841775 * a - 1.291485548 * b) ** 3;
  const lin = [
    4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_,
    -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_,
    -0.0041960863 * l_ - 0.7034186147 * m_ + 1.707614701 * s_,
  ];
  const enc = (x: number) => {
    const v = Math.min(1, Math.max(0, x));
    return v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055;
  };
  return [enc(lin[0]), enc(lin[1]), enc(lin[2])];
}
