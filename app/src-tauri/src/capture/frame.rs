//! Pure frame processing: region math, crop/downscale, change detection, rate limiting and the
//! wire format of a backdrop frame (Phase 1 Task 5).

use xxhash_rust::xxh3::xxh3_64;

/// Logical width of the island window (tauri.conf.json).
pub const WINDOW_W: f64 = 640.0;
/// Largest island shape (`alarm` 440×210), logical px.
pub const MAX_PILL_W: f64 = 440.0;
pub const MAX_PILL_H: f64 = 210.0;
/// Pill top edge below the window top, logical px.
pub const PILL_TOP: f64 = 10.0;
/// Backdrop kept around the max pill so refraction can sample outside the shape, logical px.
pub const MARGIN: f64 = 24.0;
/// Downscale factor applied to captured pixels.
pub const FACTOR: u32 = 2;
/// Bytes before the pixels in a frame message: w:u16, h:u16, t_capture_us:u64 (all LE).
pub const HEADER_LEN: usize = 12;

/// A rectangle in physical px of the captured monitor (origin = monitor top-left).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// The rect the backdrop texture covers, logical px relative to the island window.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct BackdropRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Capture region for the island: max pill rect + margin, in physical px of the monitor whose
/// virtual-screen rect is `monitor` (x, y, w, h), clipped to it, w/h aligned down to `factor`.
/// Also returns the rect the clipped region covers in window-logical px (for the shader).
pub fn island_region(
    scale: f64,
    window_pos: (i32, i32),
    monitor: (i32, i32, u32, u32),
    factor: u32,
) -> (Region, BackdropRect) {
    island_region_sized(scale, window_pos, monitor, factor, (MAX_PILL_W, MAX_PILL_H))
}

pub fn island_region_sized(
    scale: f64, window_pos: (i32, i32), monitor: (i32, i32, u32, u32), factor: u32,
    pill: (f64, f64),
) -> (Region, BackdropRect) {
    let lx = (WINDOW_W - pill.0) / 2.0 - MARGIN;
    let ly = PILL_TOP - MARGIN;
    let (lw, lh) = (pill.0 + 2.0 * MARGIN, pill.1 + 2.0 * MARGIN);
    let (wx, wy) = (window_pos.0 - monitor.0, window_pos.1 - monitor.1);
    let x0 = (wx + (lx * scale).floor() as i32).max(0);
    let y0 = (wy + (ly * scale).floor() as i32).max(0);
    let x1 = (wx + ((lx + lw) * scale).ceil() as i32).min(monitor.2 as i32);
    let y1 = (wy + ((ly + lh) * scale).ceil() as i32).min(monitor.3 as i32);
    let f = factor.max(1);
    let w = ((x1 - x0).max(0) as u32) / f * f;
    let h = ((y1 - y0).max(0) as u32) / f * f;
    let rect = BackdropRect {
        x: (x0 - wx) as f64 / scale,
        y: (y0 - wy) as f64 / scale,
        w: w as f64 / scale,
        h: h as f64 / scale,
    };
    (Region { x: x0, y: y0, w, h }, rect)
}

/// Intersect `region` with a `src_w`×`src_h` image.
pub fn clip(region: Region, src_w: u32, src_h: u32) -> Region {
    let (sw, sh) = (src_w as i64, src_h as i64);
    let x0 = (region.x as i64).clamp(0, sw);
    let y0 = (region.y as i64).clamp(0, sh);
    let x1 = (region.x as i64 + region.w as i64).clamp(0, sw);
    let y1 = (region.y as i64 + region.h as i64).clamp(0, sh);
    Region { x: x0 as i32, y: y0 as i32, w: (x1 - x0).max(0) as u32, h: (y1 - y0).max(0) as u32 }
}

/// Crop `region` (clipped to the source) out of a BGRA8 image whose rows are `stride` bytes
/// apart and box-filter it down by `factor` (partial edge blocks dropped). Returns tight BGRA8.
#[allow(dead_code)] // plan API; the capture pipeline reuses a buffer via `crop_downscale_into`
pub fn crop_downscale(src: &[u8], src_w: u32, src_h: u32, stride: u32, region: Region, factor: u32) -> Vec<u8> {
    let mut out = Vec::new();
    crop_downscale_into(src, src_w, src_h, stride, region, factor, &mut out);
    out
}

/// `crop_downscale` into a reusable buffer; returns the output (w, h).
pub fn crop_downscale_into(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    stride: u32,
    region: Region,
    factor: u32,
    out: &mut Vec<u8>,
) -> (u32, u32) {
    let r = clip(region, src_w, src_h);
    let f = factor.max(1) as usize;
    let (ow, oh) = (r.w as usize / f, r.h as usize / f);
    out.clear();
    out.resize(ow * oh * 4, 0);
    let (stride, n) = (stride as usize, (f * f) as u32);
    let (rx, ry) = (r.x as usize, r.y as usize);
    let mut sums = vec![0u32; ow * 4];
    for oy in 0..oh {
        sums.fill(0);
        for dy in 0..f {
            let start = (ry + oy * f + dy) * stride + rx * 4;
            let row = &src[start..start + ow * f * 4];
            for (s, block) in sums.as_chunks_mut::<4>().0.iter_mut().zip(row.chunks_exact(f * 4)) {
                for px in block.as_chunks::<4>().0 {
                    s[0] += px[0] as u32;
                    s[1] += px[1] as u32;
                    s[2] += px[2] as u32;
                    s[3] += px[3] as u32;
                }
            }
        }
        for (d, s) in out[oy * ow * 4..(oy + 1) * ow * 4].iter_mut().zip(&sums) {
            *d = ((s + n / 2) / n) as u8;
        }
    }
    (ow as u32, oh as u32)
}

/// Passes a frame only when its xxh3-64 differs from the previous one.
#[derive(Debug, Default)]
pub struct FrameGate {
    last: Option<u64>,
}

impl FrameGate {
    pub fn should_send(&mut self, bytes: &[u8]) -> bool {
        let h = xxh3_64(bytes);
        let changed = self.last != Some(h);
        self.last = Some(h);
        changed
    }

    /// Forget the last hash so the next frame always passes (new subscriber / mode change).
    pub fn reset(&mut self) {
        self.last = None;
    }
}

/// Admits at most `fps` frames per second on average, tolerating arrival jitter.
#[derive(Debug)]
pub struct RateLimiter {
    interval_us: u64,
    last_us: Option<u64>,
    next_us: u64,
}

impl RateLimiter {
    pub fn new(fps: u32) -> Self {
        let mut rl = RateLimiter { interval_us: 0, last_us: None, next_us: 0 };
        rl.set_fps(fps);
        rl
    }

    /// Change the target rate (clamped to 1..=240); re-bases the next slot on the last admit.
    pub fn set_fps(&mut self, fps: u32) {
        self.interval_us = 1_000_000 / fps.clamp(1, 240) as u64;
        if let Some(last) = self.last_us {
            self.next_us = last + self.interval_us;
        }
    }

    pub fn interval_us(&self) -> u64 {
        self.interval_us
    }

    /// Earliest time (µs) the next frame is due.
    pub fn next_due_us(&self) -> u64 {
        self.next_us
    }

    pub fn admit(&mut self, now_us: u64) -> bool {
        let slack = (self.interval_us / 4).min(2_000);
        if self.last_us.is_some() && now_us + slack < self.next_us {
            return false;
        }
        let behind = self.last_us.is_none() || now_us > self.next_us + self.interval_us;
        self.next_us = if behind { now_us } else { self.next_us } + self.interval_us;
        self.last_us = Some(now_us);
        true
    }
}

/// One backdrop frame on the wire: `HEADER_LEN` header + tight BGRA8 pixels.
pub fn encode_frame(w: u32, h: u32, t_capture_us: u64, pixels: &[u8]) -> Vec<u8> {
    let mut msg = Vec::with_capacity(HEADER_LEN + pixels.len());
    msg.extend_from_slice(&(w.min(u16::MAX as u32) as u16).to_le_bytes());
    msg.extend_from_slice(&(h.min(u16::MAX as u32) as u16).to_le_bytes());
    msg.extend_from_slice(&t_capture_us.to_le_bytes());
    msg.extend_from_slice(pixels);
    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 4×4 BGRA image where pixel (x, y) = [x*10+y, 100+x, 200-y, 255], rows padded to `stride`.
    fn pattern(stride: u32) -> Vec<u8> {
        let mut v = vec![0xAA; (stride * 4) as usize];
        for y in 0..4u32 {
            for x in 0..4u32 {
                let i = (y * stride + x * 4) as usize;
                v[i..i + 4].copy_from_slice(&[(x * 10 + y) as u8, (100 + x) as u8, (200 - y) as u8, 255]);
            }
        }
        v
    }

    #[test]
    fn crop_downscale_averages_2x2_blocks() {
        let src = pattern(16);
        let out = crop_downscale(&src, 4, 4, 16, Region { x: 0, y: 0, w: 4, h: 4 }, 2);
        assert_eq!(out.len(), 2 * 2 * 4);
        // Block (0,0): B = mean(0,1,10,11) = 5.5 → 6 (round half up), G = 100.5 → 101, R = 199.5 → 200.
        assert_eq!(&out[0..4], &[6, 101, 200, 255]);
        assert_eq!(&out[4..8], &[26, 103, 200, 255]);
        assert_eq!(&out[8..12], &[8, 101, 198, 255]);
        assert_eq!(&out[12..16], &[28, 103, 198, 255]);
    }

    #[test]
    fn crop_downscale_respects_stride_padding() {
        let tight = crop_downscale(&pattern(16), 4, 4, 16, Region { x: 0, y: 0, w: 4, h: 4 }, 2);
        let padded = crop_downscale(&pattern(24), 4, 4, 24, Region { x: 0, y: 0, w: 4, h: 4 }, 2);
        assert_eq!(tight.len(), 16);
        assert_eq!(tight, padded);
    }

    #[test]
    fn crop_downscale_crops_to_region() {
        let out = crop_downscale(&pattern(24), 4, 4, 24, Region { x: 2, y: 2, w: 2, h: 2 }, 2);
        assert_eq!(out, vec![28, 103, 198, 255]);
        let one = crop_downscale(&pattern(24), 4, 4, 24, Region { x: 1, y: 3, w: 1, h: 1 }, 1);
        assert_eq!(one, vec![13, 101, 197, 255]);
    }

    #[test]
    fn crop_downscale_clips_region_to_source() {
        let out = crop_downscale(&pattern(16), 4, 4, 16, Region { x: -2, y: -2, w: 4, h: 4 }, 2);
        assert_eq!(out, vec![6, 101, 200, 255]);
    }

    #[test]
    fn frame_gate_detects_changes() {
        let mut g = FrameGate::default();
        let mut bytes = vec![7u8; 1024];
        assert!(g.should_send(&bytes));
        assert!(!g.should_send(&bytes));
        bytes[513] ^= 1;
        assert!(g.should_send(&bytes));
        assert!(!g.should_send(&bytes));
    }

    #[test]
    fn frame_gate_reset_forces_next_send() {
        let mut g = FrameGate::default();
        let bytes = [1u8; 64];
        assert!(g.should_send(&bytes));
        g.reset();
        assert!(g.should_send(&bytes));
    }

    #[test]
    fn island_region_at_scale_1_clips_top_margin() {
        // Window at (640, 0) on a 1920×1080 primary monitor. Logical capture rect relative to the
        // window is x=76, y=−14, 488×258; the 14 px above the screen top are clipped.
        let (r, rect) = island_region(1.0, (640, 0), (0, 0, 1920, 1080), 2);
        assert_eq!(r, Region { x: 716, y: 0, w: 488, h: 244 });
        assert_eq!(rect, BackdropRect { x: 76.0, y: 0.0, w: 488.0, h: 244.0 });
    }

    #[test]
    fn settings_capture_covers_larger_glass_and_preserves_regular_budget() {
        let (r, rect) = island_region_sized(1.0, (640, 0), (0, 0, 1920, 1080), 2, (480.0, 420.0));
        assert_eq!(r, Region { x: 696, y: 0, w: 528, h: 454 });
        assert!(rect.x < 80.0 && rect.x + rect.w > 560.0);
        assert!(rect.y + rect.h > 430.0);
        let (regular, _) = island_region(1.0, (640, 0), (0, 0, 1920, 1080), 2);
        assert!(regular.w * regular.h < r.w * r.h);
    }

    #[test]
    fn island_region_at_scale_1_25_is_physical_and_factor_aligned() {
        // 2560-wide monitor, window centred: x = (2560 − 800) / 2 = 880.
        let (r, rect) = island_region(1.25, (880, 0), (0, 0, 2560, 1400), 2);
        // x = 880 + 95, w = 610, h = 322.5 − 17.5 → 305 → aligned down to 304.
        assert_eq!(r, Region { x: 975, y: 0, w: 610, h: 304 });
        assert_eq!(r.w % 2, 0);
        assert_eq!(r.h % 2, 0);
        assert!((rect.x - 76.0).abs() < 1e-9);
        assert!((rect.w - 488.0).abs() < 1e-9);
        assert!((rect.h - 304.0 / 1.25).abs() < 1e-9);
    }

    #[test]
    fn island_region_is_monitor_relative() {
        // Primary monitor not at the virtual-screen origin.
        let (r, _) = island_region(1.0, (2560, 100), (1920, 0, 1920, 1080), 2);
        assert_eq!(r.x, 2560 - 1920 + 76);
        assert_eq!(r.y, 100 - 14);
        assert_eq!(r.h, 258);
    }

    #[test]
    fn frame_header_layout() {
        let msg = encode_frame(488, 244, 0x0102_0304_0506_0708, &[9, 9, 9, 9]);
        assert_eq!(&msg[0..2], &488u16.to_le_bytes());
        assert_eq!(&msg[2..4], &244u16.to_le_bytes());
        assert_eq!(&msg[4..12], &0x0102_0304_0506_0708u64.to_le_bytes());
        assert_eq!(&msg[12..], &[9, 9, 9, 9]);
        assert_eq!(HEADER_LEN, 12);
    }

    #[test]
    fn rate_limiter_admits_at_interval() {
        let mut rl = RateLimiter::new(30);
        assert!(rl.admit(0));
        assert!(!rl.admit(10_000));
        assert!(rl.admit(33_400));
        rl.set_fps(60);
        assert!(!rl.admit(40_000));
        assert!(rl.admit(50_100));
    }

    #[test]
    fn rate_limiter_halves_jittery_60hz_to_30() {
        let mut rl = RateLimiter::new(30);
        let jitter = [300i64, 0, -500, 0, 400, -200];
        let admitted = (0..60)
            .filter(|&i| rl.admit((i as i64 * 16_667 + jitter[i % 6] + 1_000) as u64))
            .count();
        assert!((29..=31).contains(&admitted), "admitted {admitted}");
    }

    #[test]
    fn rate_limiter_clamps_fps() {
        let mut rl = RateLimiter::new(0);
        assert!(rl.admit(0));
        assert!(!rl.admit(500_000));
        assert!(rl.admit(1_000_000));
    }
}
