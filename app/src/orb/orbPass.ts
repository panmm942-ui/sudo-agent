import type { FrameInfo, RenderPass } from "../gl/renderer";
import { ORB_FRAG, ORB_SWIRL_HZ, ORB_VERT } from "./OrbCanvas";
import { orbAccent, orbSecondary, ORB_SCALE_GAIN, type OrbMode, type OrbUniforms } from "./features";

/** Mode cross-fade time constant (s). */
const MODE_TAU = 0.35;
/** Gentle drift of the metaballs outside thinking (rad/s). */
const IDLE_SWIRL = 0.12;

const UNIFORMS = [
  "u_center", "u_box", "u_time", "u_swirl", "u_think", "u_scale",
  "u_turbulence", "u_ripple", "u_accent", "u_opacity", "u_flow", "u_accent2",
] as const;
type UniformName = (typeof UNIFORMS)[number];

/** Shader box half-size per slot size: the disk (radius 0.43 box) fills ~75 % of the slot and its halo spills past it. */
const BOX_PER_SLOT = 0.75;

/** Square slot the orb sits in, window-relative logical px. */
export type OrbSlot = { x: number; y: number; size: number };

const REST: OrbUniforms = { scale: 1, hue: 70, turbulence: 0.2, ripple: 0 };

/**
 * The voice orb as a pass of the glass renderer (one WebGL context for the whole island).
 * Drawn scissored to its slot; the shader is compiled in `init`, i.e. at startup.
 */
export class OrbPass implements RenderPass {
  slot: OrbSlot | null = null;
  mode: OrbMode = "listening";
  uniforms: OrbUniforms = { ...REST };
  opacity = 0;

  private gl: WebGL2RenderingContext | null = null;
  private program: WebGLProgram | null = null;
  private vao: WebGLVertexArrayObject | null = null;
  private loc = {} as Record<UniformName, WebGLUniformLocation | null>;
  private lastTime: number | null = null;
  private think = 0;
  private swirl = 0;
  /** Flow clock: advances faster with voice energy, so the inner current never jumps when the speed changes. */
  private flow = 0;

  init(gl: WebGL2RenderingContext): void {
    this.gl = gl;
    this.program = link(gl, ORB_VERT, ORB_FRAG);
    this.vao = gl.createVertexArray();
    for (const n of UNIFORMS) this.loc[n] = gl.getUniformLocation(this.program, n);
  }

  /** True while the listening ↔ thinking cross-fade is still moving. */
  get transitioning(): boolean {
    const target = this.mode === "thinking" ? 1 : 0;
    return Math.abs(this.think - target) > 1e-3;
  }

  /** Snaps the mode cross-fade (frozen goldens). */
  snap(): void {
    this.think = this.mode === "thinking" ? 1 : 0;
  }

  draw(gl: WebGL2RenderingContext, frame: FrameInfo): void {
    const t = frame.timeMs / 1000;
    const dt = this.lastTime === null ? 0 : Math.min(Math.max(t - this.lastTime, 0), 0.1);
    this.lastTime = t;
    const target = this.mode === "thinking" ? 1 : 0;
    this.think += (target - this.think) * (1 - Math.exp(-dt / MODE_TAU));
    this.swirl += dt * (IDLE_SWIRL + this.think * (2 * Math.PI * ORB_SWIRL_HZ - IDLE_SWIRL));
    const energy = Math.min(Math.max((this.uniforms.scale - 1) / ORB_SCALE_GAIN, 0), 1);
    this.flow += dt * (0.3 + 1.8 * energy + 0.7 * this.think);

    const slot = this.slot;
    if (!slot || this.opacity <= 0 || !this.program) return;
    const { dpr } = frame;
    const ph = Math.round(frame.viewport.h * dpr);
    const cx = (slot.x + slot.size / 2) * dpr;
    const cy = ph - (slot.y + slot.size / 2) * dpr;
    const half = slot.size * BOX_PER_SLOT * dpr;
    const u = this.uniforms;
    const loc = this.loc;

    gl.enable(gl.SCISSOR_TEST);
    gl.scissor(Math.floor(cx - half), Math.floor(cy - half), Math.ceil(2 * half) + 1, Math.ceil(2 * half) + 1);
    gl.useProgram(this.program);
    gl.bindVertexArray(this.vao);
    gl.uniform2f(loc.u_center, cx, cy);
    gl.uniform1f(loc.u_box, half);
    gl.uniform1f(loc.u_time, t);
    gl.uniform1f(loc.u_swirl, this.swirl);
    gl.uniform1f(loc.u_think, this.think);
    gl.uniform1f(loc.u_scale, u.scale);
    gl.uniform1f(loc.u_turbulence, u.turbulence);
    gl.uniform1f(loc.u_ripple, u.ripple);
    gl.uniform3fv(loc.u_accent, orbAccent(u.hue));
    gl.uniform1f(loc.u_opacity, this.opacity);
    gl.uniform1f(loc.u_flow, this.flow);
    gl.uniform3fv(loc.u_accent2, orbSecondary(u.hue));
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.disable(gl.SCISSOR_TEST);
  }

  dispose(): void {
    if (this.gl && !this.gl.isContextLost()) {
      this.gl.deleteProgram(this.program);
      this.gl.deleteVertexArray(this.vao);
    }
    this.gl = this.program = this.vao = null;
  }
}

function link(gl: WebGL2RenderingContext, vs: string, fs: string): WebGLProgram {
  const compile = (type: number, src: string) => {
    const sh = gl.createShader(type)!;
    gl.shaderSource(sh, src);
    gl.compileShader(sh);
    if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) throw new Error(`orb shader: ${gl.getShaderInfoLog(sh)}`);
    return sh;
  };
  const p = gl.createProgram()!;
  const v = compile(gl.VERTEX_SHADER, vs);
  const f = compile(gl.FRAGMENT_SHADER, fs);
  gl.attachShader(p, v);
  gl.attachShader(p, f);
  gl.linkProgram(p);
  gl.deleteShader(v);
  gl.deleteShader(f);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(`orb link: ${gl.getProgramInfoLog(p)}`);
  return p;
}
