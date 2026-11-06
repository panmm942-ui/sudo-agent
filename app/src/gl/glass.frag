#version 300 es
// Liquid glass pill (Apple-style): a convex squircle bezel refracts the backdrop by Snell's law,
// a light frost and vibrancy keep it clear, an adaptive veil keeps white text legible, and a
// crisp specular rim plus a soft lift shadow give it body. Lengths in logical px, y down.
precision highp float;

uniform vec2 u_viewport;      // window size, logical px
uniform float u_dpr;
uniform vec2 u_center;        // pill centre
uniform vec2 u_size;          // pill w, h
uniform float u_radius;
uniform sampler2D u_backdrop;
uniform int u_hasBackdrop;
uniform int u_bgra;           // backdrop bytes are BGRA (raw WGC frames)
uniform vec4 u_backdropRect;  // window-space rect covered by the backdrop texture (x, y, w, h)
uniform vec4 u_tint;          // sRGB-encoded tint and blend strength
uniform float u_colored;     // smoothly eased track tint blend (0..1)
uniform float u_outline;
uniform float u_darkening;
uniform float u_opacity;
uniform float u_blur;
uniform float u_glisse;      // 0 = Sudo glass, 1 = Glisse liquid glass (eased between)

out vec4 outColor;

const vec2 LIGHT = vec2(-0.55, -0.835);  // unit vector towards the key light (top-left)
const float IOR = 1.5;
const float THICK = 3.0;      // height/bezel ratio of the convex profile (steeper = wider lens)
const float LENS = 0.9;       // edge displacement as a fraction of the bezel width
const float MAG = 0.985;      // faint interior magnification
const float FROST = 2.6;      // frost radius, logical px
const float SAT = 1.4;
const float LIFT = 1.03;
const float TEXT_LUMA = 0.17; // linear luminance under white text for >= 4.5:1
const float SHADOW_Y = 5.0;
const float SHADOW_A = 0.18;
const float SHADOW_IN = -6.0;
const float SHADOW_OUT = 24.0;
const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);

// Golden-angle disc, unit radius.
const vec2 DISC[12] = vec2[12](
  vec2(0.2041, 0.0), vec2(-0.2607, 0.2388), vec2(0.0399, -0.4547), vec2(0.3286, 0.4287),
  vec2(-0.6030, -0.1067), vec2(0.5712, -0.3634), vec2(-0.1911, 0.7108), vec2(-0.3649, -0.7014),
  vec2(0.7905, 0.2887), vec2(-0.8225, 0.3396), vec2(0.3958, -0.8476), vec2(0.2931, 0.9340));

float sdRoundedRect(vec2 p, vec2 halfSize, float r) {
  vec2 q = abs(p) - (halfSize - r);
  return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

// Outward unit gradient of sdRoundedRect; inside the inner rectangle it falls back to the
// nearest axis.
vec2 sdNormal(vec2 p, vec2 halfSize, float r) {
  vec2 q = abs(p) - (halfSize - r);
  vec2 n;
  if (max(q.x, q.y) > 0.0) {
    n = normalize(max(q, 0.0) + 1e-6);
  } else {
    n = q.x > q.y ? vec2(1.0, 0.0) : vec2(0.0, 1.0);
  }
  return n * sign(p + 1e-6);
}

vec3 toLinear(vec3 c) { return pow(c, vec3(2.2)); }
vec3 toSrgb(vec3 c) { return pow(clamp(c, 0.0, 1.0), vec3(1.0 / 2.2)); }

vec3 tap(vec2 p) {
  vec2 uv = (p - u_backdropRect.xy) / u_backdropRect.zw;
  vec3 c = texture(u_backdrop, clamp(uv, 0.0, 1.0)).rgb;
  return u_bgra == 1 ? c.bgr : c;
}

vec3 frost(vec2 p) {
  // Spreading a few sharp taps farther apart produces ghosts/aliasing, not a smooth blur.
  // Prefilter each tap at the corresponding texture footprint before widening the disc.
  // Keep the original level-zero frost at 0%, including the clear glass rim/probes.
  if (u_blur > 0.0) {
    vec2 texelsPerPx = vec2(textureSize(u_backdrop, 0)) / u_backdropRect.zw;
    float lod = log2(max(1.0, u_blur * 20.0 * max(texelsPerPx.x, texelsPerPx.y)));
    vec3 s = textureLod(u_backdrop, clamp((p - u_backdropRect.xy) / u_backdropRect.zw, 0.0, 1.0), lod).rgb;
    for (int i = 0; i < 12; i++) {
      vec2 q = p + DISC[i] * (FROST + u_blur * 26.0);
      s += textureLod(u_backdrop, clamp((q - u_backdropRect.xy) / u_backdropRect.zw, 0.0, 1.0), lod).rgb;
    }
    return (u_bgra == 1 ? s.bgr : s) / 13.0;
  }
  vec3 s = tap(p);
  for (int i = 0; i < 12; i++) s += tap(p + DISC[i] * FROST);
  return s / 13.0;
}

// Lateral refraction (normalised to 1 at the rim) across a convex squircle bezel
// h(x) = (1 - (1 - x)^4)^(1/4), x = 0 at the rim .. 1 where the flat top begins.
float lensMag(float x) {
  float y = 1.0 - clamp(x, 0.0, 1.0);
  float y3 = y * y * y;
  float slope = THICK * y3 / pow(max(1.0 - y3 * y, 1e-4), 0.75);
  vec3 n = normalize(vec3(slope, 0.0, 1.0));
  vec3 r = refract(vec3(0.0, 0.0, -1.0), n, 1.0 / IOR);
  return (-r.x / max(-r.z, 1e-3)) / 1.118;
}

void main() {
  vec2 p = vec2(gl_FragCoord.x / u_dpr, u_viewport.y - gl_FragCoord.y / u_dpr);
  vec2 halfSize = u_size * 0.5;
  float r = min(u_radius, min(halfSize.x, halfSize.y));
  vec2 lp = p - u_center;
  float d = sdRoundedRect(lp, halfSize, r);
  float aa = max(fwidth(d), 1e-4);
  float coverage = clamp(0.5 - d / aa, 0.0, 1.0);

  // Lift: a wide, soft shadow below the pill; exactly zero beyond SHADOW_OUT.
  float s = 1.0 - smoothstep(SHADOW_IN, SHADOW_OUT, sdRoundedRect(lp - vec2(0.0, SHADOW_Y), halfSize, r));
  float shadow = SHADOW_A * s * s;
  if (coverage <= 0.0) {
    outColor = vec4(0.0, 0.0, 0.0, shadow * u_opacity);
    return;
  }

  float depth = max(-d, 0.0);
  vec2 n = sdNormal(lp, halfSize, r);
  float bezel = clamp(min(u_size.x, u_size.y) * 0.35, 12.0, 36.0);
  float m = lensMag(depth / bezel);
  float facing = dot(n, LIGHT);

  vec3 col;
  float alpha = 1.0;
  if (u_hasBackdrop == 1) {
    // The rim shows the backdrop from just beyond the edge, squeezed in by the curved bezel.
    vec2 q = u_center + lp * MAG + n * (m * bezel * LENS);
    col = toLinear(frost(q));
    // Faint dispersion at the outermost pixel only.
    float disp = smoothstep(0.75, 1.0, m) * 0.5;
    col.r = mix(col.r, toLinear(tap(q + n * 1.0)).r, disp);
    col.b = mix(col.b, toLinear(tap(q - n * 1.0)).b, disp);

    float l = dot(col, LUMA);
    col = max(mix(vec3(l), col, SAT), 0.0) * LIFT;

    // Adaptive veil from a few wide probes over the shape: darken (hue-preserving) over bright
    // content so white text keeps contrast; lift slightly over very dark content.
    float sum = 0.0;
    float peak = 0.0;
    for (int i = 0; i < 7; i++) {
      vec2 o = i == 0 ? vec2(0.0) : vec2(cos(float(i) * 1.0472), sin(float(i) * 1.0472)) * vec2(0.62, 0.45);
      float pl = dot(toLinear(tap(u_center + o * halfSize)), LUMA) * LIFT;
      sum += pl;
      peak = max(peak, pl);
    }
    float ref = mix(sum / 7.0, peak, 0.2);
    float veil = clamp(TEXT_LUMA / max(ref, 1e-4), 0.28, 1.0);
    col *= mix(veil, 1.0, 0.45 * m);
    // Soft knee on local highlights that the shape-wide veil left too bright for white text.
    float knee = TEXT_LUMA;
    float lv = dot(col, LUMA);
    col *= mix(1.0, knee / max(lv, knee) * (1.0 + 0.15 * max(lv - knee, 0.0) / max(lv, 1e-4)), 1.0 - m);
    col +=(1.0 - col) * 0.015 * (1.0 - smoothstep(0.01, 0.08, ref));
    // A 98%-opaque song tint otherwise erases the sampled desktop even when blur is enabled.
    // Gradually leave room for its softened colour; preserve the original material at 0%.
    float tint = mix(u_tint.a, min(u_tint.a, 0.55), u_blur);
    col = mix(col, toLinear(u_tint.rgb), tint * u_colored);
  } else {
    col = toLinear(u_tint.rgb);
    alpha = u_tint.a;
  }
  col *= 1.0 - u_darkening;

  // Edge body: internal reflection darkens a thin band just inside the rim.
  float band = smoothstep(0.8, 2.2, depth) * (1.0 - smoothstep(2.2, 7.0, depth));
  col *= 1.0 - 0.16 * band;

  // Light: a crisp rim strongest where the edge faces the key light, a weaker exit highlight on
  // the opposite edge, and a soft glow across the bezel.
  float key = pow(max(facing, 0.0), 4.0);
  float back = pow(max(-facing, 0.0), 4.0);
  float core = 1.0 - smoothstep(0.0, 1.25, depth);
  float halo = 1.0 - smoothstep(0.0, 5.0, depth);
  float spec = core * (1.1 * key + 0.55 * back + 0.14) + halo * (0.3 * key + 0.12 * back);
  float glow = m * (0.04 + 0.06 * max(facing, 0.0));
  float add = spec + glow;
  // Border treatments share the exact same refracted/frosted body. Only the edge changes.
  if (u_outline > 0.5) add *= u_outline > 3.5 ? 0.04 : 0.18;
  col += vec3(add);
  alpha = alpha + clamp(add, 0.0, 1.0) * (1.0 - alpha);

  vec3 rgb = toSrgb(col);
  float contour = 1.0 - smoothstep(0.55, 1.45, depth);
  if (u_outline > 0.5 && u_outline < 1.5) rgb = mix(rgb, vec3(0.98), contour * 0.78);
  if (u_outline > 1.5 && u_outline < 2.5) rgb = mix(rgb, vec3(0.95, 0.94, 0.91), contour * (0.30 + 0.28 * key));
  if (u_outline > 2.5 && u_outline < 3.5) {
    rgb = mix(rgb, vec3(0.28, 0.30, 0.33), contour * 0.78);
    float inner = smoothstep(1.0, 1.6, depth) * (1.0 - smoothstep(1.6, 2.3, depth));
    rgb = mix(rgb, vec3(0.75), inner * 0.14 * key);
  }
  float a = alpha * coverage;

  // Glisse liquid glass (after Glisse by Kelvin Zhang, MIT): within a band around the rim
  // each point shows content from further in, by the slope of a shallow dome there; the
  // channels bend by slightly different amounts (dispersion); the bent, frosted and
  // saturated backdrop is covered by Glisse's dark glass (rgb 40 at 62 %), lit by a glow
  // along the light's axis and a thin lit rim, with a 0.5 px edge line.
  if (u_glisse > 0.0) {
    const float G_BEND = 0.45;
    const float G_DISPERSION = 0.4;
    const float G_RIM_SHINE = 0.13;
    const vec3 G_GLASS = vec3(40.0 / 255.0);
    const float G_GLASS_A = 0.62;
    float gBand = clamp(min(u_size.x, u_size.y) * 0.3, 10.0, 28.0);
    float t = clamp(1.0 - depth / gBand, 0.0, 0.999);
    float dome = t / sqrt(1.0 - t * t);
    float pull = gBand * G_BEND * dome / (1.0 + dome);
    vec3 g;
    float gA = 1.0;
    if (u_hasBackdrop == 1) {
      vec2 base = p - n * pull;
      g.r = frost(base - n * pull * 0.2 * G_DISPERSION).r;
      g.g = frost(base - n * pull * 0.1 * G_DISPERSION).g;
      g.b = frost(base).b;
      float gl = dot(g, LUMA);
      g = clamp(mix(vec3(gl), g, 1.5), 0.0, 1.0);
      // With a song playing, Glisse's dark glass takes the song's color (as Sudo's glass does).
      vec3 gGlass = mix(G_GLASS, u_tint.rgb, u_colored);
      g = mix(g, gGlass, mix(G_GLASS_A, max(G_GLASS_A, u_tint.a), u_colored));
    } else {
      g = mix(G_GLASS * 0.9, u_tint.rgb, u_colored);
      gA = 0.94;
    }
    float gKey = max(facing, 0.0);
    float gRim = (1.0 - smoothstep(0.0, 1.1, depth)) * G_RIM_SHINE * (0.55 + 1.6 * pow(gKey, 2.0));
    float gGlow = t * t * 0.05 * gKey;
    float gEdge = (1.0 - smoothstep(0.0, 0.5, depth)) * 0.08;
    g = g + vec3(gRim + gGlow) + (1.0 - g) * gEdge;
    float ga = gA * coverage;
    rgb = mix(rgb, clamp(g, 0.0, 1.0), u_glisse);
    a = mix(a, ga, u_glisse);
    shadow *= 1.0 - 0.65 * u_glisse;
  }
  outColor = vec4(rgb * a, a + shadow * (1.0 - a)) * u_opacity;
}
