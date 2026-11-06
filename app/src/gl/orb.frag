#version 300 es
// Voice orb: a noise-displaced metaball disk lit like a liquid bead (fresnel rim,
// soft top-left glint, inner flow), one OKLCH accent hue, premultiplied alpha output.
precision highp float;

uniform vec2 u_center;       // orb centre, device px (gl_FragCoord space)
uniform float u_box;         // half-size of the orb's square, device px
uniform float u_time;        // flow time, s
uniform float u_swirl;       // accumulated swirl angle, rad (0.15 Hz while thinking)
uniform float u_think;       // 0 listening/speaking .. 1 thinking
uniform float u_scale;       // 1 .. 1.35
uniform float u_turbulence;  // 0.2 .. 1
uniform float u_ripple;      // 1 on onset, decays to 0
uniform vec3 u_accent;       // linear sRGB of OKLCH(0.80 0.11 hue)
uniform float u_opacity;     // content-transition fade
uniform float u_flow;        // flow clock: speeds up with voice energy and while thinking
uniform vec3 u_accent2;      // second, deeper hue for the inner two-tone current

out vec4 fragColor;

const float BASE_R = 0.43;
// Same key light as the glass rim: top-left (-0.5, -0.8) in screen space, y flipped here.
const vec3 LIGHT = vec3(-0.4472, 0.7155, 0.5367);

mat2 rot(float a) {
  float c = cos(a), s = sin(a);
  return mat2(c, -s, s, c);
}

// Dave Hoskins' hash: stable for large inputs (no sin()).
vec3 hash33(vec3 p) {
  p = fract(p * vec3(0.1031, 0.1030, 0.0973));
  p += dot(p, p.yxz + 33.33);
  return -1.0 + 2.0 * fract((p.xxy + p.yxx) * p.zyx);
}

float gnoise(vec3 p) {
  vec3 i = floor(p);
  vec3 f = fract(p);
  vec3 u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
  float n000 = dot(hash33(i), f);
  float n100 = dot(hash33(i + vec3(1, 0, 0)), f - vec3(1, 0, 0));
  float n010 = dot(hash33(i + vec3(0, 1, 0)), f - vec3(0, 1, 0));
  float n110 = dot(hash33(i + vec3(1, 1, 0)), f - vec3(1, 1, 0));
  float n001 = dot(hash33(i + vec3(0, 0, 1)), f - vec3(0, 0, 1));
  float n101 = dot(hash33(i + vec3(1, 0, 1)), f - vec3(1, 0, 1));
  float n011 = dot(hash33(i + vec3(0, 1, 1)), f - vec3(0, 1, 1));
  float n111 = dot(hash33(i + vec3(1, 1, 1)), f - vec3(1, 1, 1));
  return mix(
    mix(mix(n000, n100, u.x), mix(n010, n110, u.x), u.y),
    mix(mix(n001, n101, u.x), mix(n011, n111, u.x), u.y),
    u.z);
}

float smin(float a, float b, float k) {
  float h = max(k - abs(a - b), 0.0) / k;
  return min(a, b) - h * h * k * 0.25;
}

/** 0 at rest .. 1 at full voice, from the spring-smoothed scale. */
float energy() { return clamp((u_scale - 1.0) / 0.35, 0.0, 1.0); }

/** Signed distance (orb units) to the displaced metaball outline. */
float orbSdf(vec2 p, float R) {
  float orbit = u_flow * 0.32 + u_swirl;
  vec2 c1 = R * 0.15 * vec2(cos(orbit), sin(orbit));
  vec2 c2 = R * 0.19 * vec2(cos(2.1 - orbit * 1.3), sin(2.1 - orbit * 1.3));
  vec2 c3 = R * 0.13 * vec2(cos(4.2 + orbit * 0.7), sin(4.2 + orbit * 0.7));
  float d = length(p - c1) - R * 0.84;
  d = smin(d, length(p - c2) - R * 0.74, R * 0.5);
  d = smin(d, length(p - c3) - R * 0.78, R * 0.5);

  vec2 q = rot(u_swirl * 0.5) * p / R;
  float amp = R * (0.014 + 0.034 * u_turbulence);
  float n = gnoise(vec3(q * 0.95, u_flow * 0.45)) + 0.3 * gnoise(vec3(q * 1.9, u_flow * 0.7 + 7.0));

  // Voice lobes: slow travelling waves around the edge that swell with loudness, so the outline
  // breathes and ripples with each syllable instead of only scaling.
  float th = atan(p.y, p.x);
  float lobes = 0.45 * sin(2.0 * th + u_flow * 0.9 + 4.0)
              + 0.40 * sin(3.0 * th - u_flow * 1.3)
              + 0.25 * sin(5.0 * th + u_flow * 1.9 + 1.3);
  float e = energy();
  return d - amp * n - R * (0.014 + 0.075 * e * e) * lobes;
}

vec3 linearToSrgb(vec3 c) {
  c = max(c, 0.0);
  return mix(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, step(0.0031308, c));
}

/** Gentle shoulder above 0.8 so bright cores roll toward white instead of clipping. */
vec3 shoulder(vec3 c) {
  vec3 over = max(c - 0.8, 0.0);
  return min(c, 0.8) + 0.2 * (1.0 - exp(-over / 0.2));
}

void main() {
  vec2 p = (gl_FragCoord.xy - u_center) / u_box;
  float r = length(p);
  float R = BASE_R * u_scale * (1.0 + 0.045 * u_ripple);
  float px = 1.0 / u_box;

  float d = orbSdf(p, R);
  // Screen-space gradient: free, and smooth enough for a shading normal (d itself is smooth).
  vec2 grad = vec2(dFdx(d), dFdy(d));
  vec2 dir = length(grad) > 1e-6 ? normalize(grad) : p / max(r, 1e-4);

  // Body: a dome whose normal tilts outward toward the rim.
  float aBody = clamp(0.5 - d / px, 0.0, 1.0);
  float depth = clamp(-d / R, 0.0, 1.0);
  float q = pow(1.0 - depth, 1.8);
  vec3 n = normalize(vec3(dir * q, sqrt(max(1.0 - q * q, 0.0)) + 0.04));
  float fresnel = pow(max(1.0 - n.z, 0.0), 2.6);

  // Inner flow: a domain-warped current of two tones that drifts with the flow clock (faster when
  // the voice is louder); the inside spins faster than the rim while thinking.
  vec2 fq = rot(u_swirl + depth * 1.4 * u_think) * p / R;
  float f1 = gnoise(vec3(fq * 1.05, u_flow * 0.22));
  float f2 = gnoise(vec3(fq * 1.9 + f1 * 0.8, u_flow * 0.31 + 3.0));
  float flow = 0.5 + 0.5 * (0.7 * f1 + 0.3 * f2);
  vec2 warp = fq + 0.7 * vec2(gnoise(vec3(fq * 1.2, u_flow * 0.35 + 11.0)), gnoise(vec3(fq * 1.2 + 5.2, u_flow * 0.35 + 17.0)));
  float current = smoothstep(0.3, 0.7, 0.5 + 0.6 * gnoise(vec3(warp * 1.35, u_flow * 0.25 + 23.0)));

  // Emissive core: near-white light at the centre (brighter with the voice), two-tone current toward the rim.
  vec3 accent = u_accent;
  vec3 tone = mix(accent, u_accent2, current * (0.75 + 0.2 * depth));
  vec3 white = vec3(1.0, 0.995, 0.985);
  float glowIn = smoothstep(0.0, 1.0, depth);
  float coreMix = (0.48 + 0.3 * energy()) * pow(glowIn, 1.6) * (0.82 + 0.36 * flow);
  vec3 col = mix(tone, white, coreMix) * (0.95 + 0.2 * flow * (0.6 + 0.4 * u_turbulence));

  // Caustic: light focused through the bead lands opposite the key light.
  vec2 focus = -LIGHT.xy * R * 0.42;
  float caustic = exp(-dot(p - focus, p - focus) / (R * R * 0.07)) * smoothstep(0.0, 0.25, depth);
  col = mix(col, mix(accent, white, 0.6) * 1.15, caustic * 0.35);

  // Rim: thin bright fresnel edge, strongest under the key light; soft glint arc top-left.
  float lit = max(dot(n, LIGHT), 0.0);
  col = mix(col, mix(accent, white, 0.25) * 1.12, fresnel * (0.55 + 0.45 * lit));
  col += white * pow(lit, 8.0) * fresnel * 0.4;

  // Translucent band between core and rim so the glass beneath reads through (volume).
  float band = smoothstep(0.02, 0.18, depth) * (1.0 - smoothstep(0.25, 0.7, depth));
  float bodyAlpha = aBody * clamp(0.9 - 0.32 * band + 0.1 * caustic + 0.5 * fresnel, 0.0, 1.0);

  // Halo outside the body, plus the outward onset ring.
  float outside = max(d, 0.0);
  float halo = exp(-outside / (R * 0.38)) * (0.22 + 0.16 * (u_scale - 1.0) / 0.35);
  float ringR = R * (1.08 + 0.6 * (1.0 - u_ripple));
  float ringW = R * (0.04 + 0.06 * (1.0 - u_ripple));
  float z = (r - ringR) / ringW;
  float ring = exp(-z * z) * u_ripple * u_ripple * 0.2;
  // smoothstep needs edge0 < edge1 (reversed edges are undefined in GLSL ES).
  halo = (halo + ring) * (1.0 - smoothstep(0.7, 1.0, r));

  vec3 premul = col * bodyAlpha + mix(accent, u_accent2, 0.35) * halo * (1.0 - bodyAlpha);
  float a = bodyAlpha + halo * (1.0 - bodyAlpha);
  vec3 c = linearToSrgb(shoulder(max(premul / max(a, 1e-5), 0.0)));
  c += (hash33(vec3(gl_FragCoord.xy, 1.0)).x) / 255.0 * 0.5;
  a *= u_opacity;
  fragColor = vec4(clamp(c, 0.0, 1.0) * a, a);
}
