import fragSrc from "../gl/orb.frag?raw";
import { orbAccent, type OrbMode, type OrbUniforms } from "./features";

/** Fullscreen triangle without buffers. */
export const ORB_VERT = `#version 300 es
void main() {
  vec2 p = vec2(gl_VertexID == 1 ? 3.0 : -1.0, gl_VertexID == 2 ? 3.0 : -1.0);
  gl_Position = vec4(p, 0.0, 1.0);
}`;

export const ORB_FRAG = fragSrc;

/** Thinking swirl rate (Hz). */
export const ORB_SWIRL_HZ = 0.15;
/** Mode cross-fade time constant (s). */
const MODE_TAU = 0.35;
/** Gentle drift of the metaballs outside thinking (rad/s). */
const IDLE_SWIRL = 0.12;

const UNIFORMS = [
  "u_center",
  "u_box",
  "u_time",
  "u_swirl",
  "u_think",
  "u_scale",
  "u_turbulence",
  "u_ripple",
  "u_accent",
  "u_opacity",
] as const;
type UniformName = (typeof UNIFORMS)[number];

export type OrbCanvasOptions = {
  /** Fixed device pixel ratio override; by default `window.devicePixelRatio` is re-read on every `resize()`. */
  dpr?: number;
  /** Keep the drawing buffer after compositing (screenshots / readPixels). */
  preserveDrawingBuffer?: boolean;
};

/** Draws the voice orb into its own transparent WebGL2 canvas (premultiplied alpha, DPR-correct). */
export class OrbCanvas {
  private readonly gl: WebGL2RenderingContext;
  private readonly program: WebGLProgram;
  private readonly vao: WebGLVertexArrayObject;
  private readonly loc: Record<UniformName, WebGLUniformLocation | null>;
  private readonly dprOverride: number | undefined;

  private lastTime: number | null = null;
  private think = 0;
  private swirl = 0;

  constructor(
    private readonly canvas: HTMLCanvasElement,
    opts: OrbCanvasOptions = {},
  ) {
    this.dprOverride = opts.dpr;
    const gl = canvas.getContext("webgl2", {
      alpha: true,
      premultipliedAlpha: true,
      antialias: false,
      depth: false,
      stencil: false,
      preserveDrawingBuffer: opts.preserveDrawingBuffer ?? false,
    });
    if (!gl) throw new Error("WebGL2 unavailable");
    this.gl = gl;
    this.program = link(gl, ORB_VERT, ORB_FRAG);
    this.vao = gl.createVertexArray()!;
    this.loc = Object.fromEntries(UNIFORMS.map((n) => [n, gl.getUniformLocation(this.program, n)])) as Record<
      UniformName,
      WebGLUniformLocation | null
    >;
    this.resize();
  }

  /** Current DPR: the override, else the live `devicePixelRatio` (changes with display scale). */
  private get dpr(): number {
    return this.dprOverride ?? (typeof window === "undefined" ? 1 : window.devicePixelRatio || 1);
  }

  /** Matches the backing store to the CSS size times the current DPR. Returns true if it changed. */
  resize(): boolean {
    const dpr = this.dpr;
    const w = Math.max(1, Math.round(this.canvas.clientWidth * dpr));
    const h = Math.max(1, Math.round(this.canvas.clientHeight * dpr));
    if (this.canvas.width === w && this.canvas.height === h) return false;
    this.canvas.width = w;
    this.canvas.height = h;
    return true;
  }

  /** True while the mode cross-fade is still moving (keep the render loop awake). */
  get transitioning(): boolean {
    return this.think > 1e-3 && this.think < 1 - 1e-3;
  }

  /** Renders one frame. `timeSec` is any monotonic clock; `opacity` fades the whole orb. */
  draw(u: OrbUniforms, mode: OrbMode, timeSec: number, opacity = 1): void {
    const dt = this.lastTime === null ? 0 : Math.min(Math.max(timeSec - this.lastTime, 0), 0.1);
    this.lastTime = timeSec;
    const target = mode === "thinking" ? 1 : 0;
    this.think += (target - this.think) * (1 - Math.exp(-dt / MODE_TAU));
    this.swirl += dt * (IDLE_SWIRL + this.think * (2 * Math.PI * ORB_SWIRL_HZ - IDLE_SWIRL));

    const { gl, loc, canvas } = this;
    gl.viewport(0, 0, canvas.width, canvas.height);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.useProgram(this.program);
    gl.bindVertexArray(this.vao);
    gl.uniform2f(loc.u_center, canvas.width / 2, canvas.height / 2);
    gl.uniform1f(loc.u_box, Math.min(canvas.width, canvas.height) / 2);
    gl.uniform1f(loc.u_time, timeSec);
    gl.uniform1f(loc.u_swirl, this.swirl);
    gl.uniform1f(loc.u_think, this.think);
    gl.uniform1f(loc.u_scale, u.scale);
    gl.uniform1f(loc.u_turbulence, u.turbulence);
    gl.uniform1f(loc.u_ripple, u.ripple);
    gl.uniform3fv(loc.u_accent, orbAccent(u.hue));
    gl.uniform1f(loc.u_opacity, opacity);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }

  dispose(): void {
    this.gl.deleteProgram(this.program);
    this.gl.deleteVertexArray(this.vao);
    this.gl.getExtension("WEBGL_lose_context")?.loseContext();
  }
}

function compile(gl: WebGL2RenderingContext, type: number, src: string): WebGLShader {
  const s = gl.createShader(type)!;
  gl.shaderSource(s, src);
  gl.compileShader(s);
  if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
    const log = gl.getShaderInfoLog(s);
    gl.deleteShader(s);
    throw new Error(`orb shader compile failed: ${log}`);
  }
  return s;
}

function link(gl: WebGL2RenderingContext, vs: string, fs: string): WebGLProgram {
  const p = gl.createProgram()!;
  const v = compile(gl, gl.VERTEX_SHADER, vs);
  const f = compile(gl, gl.FRAGMENT_SHADER, fs);
  gl.attachShader(p, v);
  gl.attachShader(p, f);
  gl.linkProgram(p);
  gl.deleteShader(v);
  gl.deleteShader(f);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(`orb program link failed: ${gl.getProgramInfoLog(p)}`);
  return p;
}
