/** Procedural bright, saturated, high-detail backdrop for the glass lab; returns a PNG data URL. */
export function paintBright(w: number, h: number): string {
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const g = c.getContext("2d")!;
  // Photo-like sky-to-meadow gradient.
  const sky = g.createLinearGradient(0, 0, 0, h);
  sky.addColorStop(0, "#7cc4ff");
  sky.addColorStop(0.55, "#fdf3d8");
  sky.addColorStop(1, "#7ed36b");
  g.fillStyle = sky;
  g.fillRect(0, 0, w, h);
  // Saturated colour blocks.
  const hues = ["#ff3b30", "#ff9500", "#ffcc00", "#34c759", "#00c7be", "#007aff", "#af52de", "#ff2d55"];
  for (let i = 0; i < 16; i++) {
    g.fillStyle = hues[i % hues.length];
    const x = ((i * 173) % 1180) + 10;
    const y = ((i * 97) % 420) + 20;
    g.beginPath();
    g.roundRect(x, y, 90 + ((i * 37) % 80), 60 + ((i * 53) % 70), 14);
    g.fill();
  }
  // Fine stripes (high frequency makes lensing visible).
  g.fillStyle = "rgba(20,20,30,0.85)";
  for (let x = 0; x < w; x += 6) g.fillRect(x, 300, 2, 40);
  for (let y = 0; y < 200; y += 5) g.fillRect(380, y, 520, 1.5);
  // Text-like lines.
  g.fillStyle = "#111";
  g.font = "600 22px sans-serif";
  const words = "The quick brown fox jumps over the lazy dog 0123456789 Liquid Glass".split(" ");
  for (let row = 0; row < 12; row++) {
    let x = (row * 41) % 60;
    for (let k = 0; k < 14; k++) {
      const word = words[(row * 5 + k) % words.length];
      g.fillText(word, x, 40 + row * 44);
      x += g.measureText(word).width + 14;
    }
  }
  // White highlight disc and a dark bar to test the adaptive veil.
  g.fillStyle = "#ffffff";
  g.beginPath();
  g.arc(640, 70, 60, 0, Math.PI * 2);
  g.fill();
  g.fillStyle = "#1a1a1a";
  g.fillRect(0, 470, w, 14);
  return c.toDataURL("image/png");
}
