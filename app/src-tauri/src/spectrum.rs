//! Bounded stereo spectrum analysis. Samples are never persisted or transmitted.
use std::f32::consts::TAU;
pub const FFT_SIZE: usize = 2048;
pub const SAMPLE_RATE: u32 = 48_000;
// Bass to treble, identical for every frame: no moving normalization or random input.
const EDGES: [f32; 8] = [40., 100., 250., 630., 1600., 4000., 10000., 20000.];
pub struct Spectrum {
    samples: [[f32; FFT_SIZE]; 2],
    window: [f32; FFT_SIZE],
    real: [f32; FFT_SIZE],
    imaginary: [f32; FFT_SIZE],
    position: usize,
    window_energy: f32,
}
impl Spectrum {
    pub fn new() -> Self {
        let window = std::array::from_fn(|i| 0.5 - 0.5 * (TAU * i as f32 / FFT_SIZE as f32).cos());
        let window_energy = window.iter().map(|v| v * v).sum::<f32>();
        Self { samples: [[0.; FFT_SIZE]; 2], window, real: [0.; FFT_SIZE], imaginary: [0.; FFT_SIZE], position: 0, window_energy }
    }
    pub fn push(&mut self, left: f32, right: f32) {
        self.samples[0][self.position] = if left.is_finite() { left.clamp(-1., 1.) } else { 0. };
        self.samples[1][self.position] = if right.is_finite() { right.clamp(-1., 1.) } else { 0. };
        self.position = (self.position + 1) & (FFT_SIZE - 1);
    }
    pub fn clear(&mut self) { self.samples = [[0.; FFT_SIZE]; 2]; }
    pub fn bands(&mut self) -> [f32; 7] {
        let mut power = [0.; 7];
        for channel in 0..2 {
            for i in 0..FFT_SIZE {
                self.real[i] = self.samples[channel][(self.position + i) & (FFT_SIZE - 1)] * self.window[i];
            }
            self.imaginary.fill(0.);
            fft(&mut self.real, &mut self.imaginary);
            for bin in 1..FFT_SIZE / 2 {
                let hz = bin as f32 * SAMPLE_RATE as f32 / FFT_SIZE as f32;
                if let Some(band) = (0..7).find(|&b| hz >= EDGES[b] && hz < EDGES[b + 1]) {
                    // Sum both stereo powers, not the waveform: opposite-phase channels cannot cancel.
                    // One-sided FFT factor 2 and stereo average 1/2 cancel here.
                    power[band] += self.real[bin] * self.real[bin] + self.imaginary[bin] * self.imaginary[bin];
                }
            }
        }
        power.map(|p| (p / (FFT_SIZE as f32 * self.window_energy)).sqrt().clamp(0., 1.))
    }
}

/// In-place radix-2 FFT, fixed scratch arrays and O(N log N) work per published frame.
fn fft(real: &mut [f32; FFT_SIZE], imaginary: &mut [f32; FFT_SIZE]) {
    let mut reverse = 0;
    for i in 1..FFT_SIZE {
        let mut bit = FFT_SIZE >> 1;
        while reverse & bit != 0 { reverse ^= bit; bit >>= 1; }
        reverse ^= bit;
        if i < reverse { real.swap(i, reverse); imaginary.swap(i, reverse); }
    }
    let mut size = 2;
    while size <= FFT_SIZE {
        let angle = -TAU / size as f32;
        let (step_i, step_r) = angle.sin_cos();
        for start in (0..FFT_SIZE).step_by(size) {
            let (mut wr, mut wi) = (1., 0.);
            for offset in 0..size / 2 {
                let a = start + offset;
                let b = a + size / 2;
                let tr = wr * real[b] - wi * imaginary[b];
                let ti = wr * imaginary[b] + wi * real[b];
                real[b] = real[a] - tr; imaginary[b] = imaginary[a] - ti;
                real[a] += tr; imaginary[a] += ti;
                (wr, wi) = (wr * step_r - wi * step_i, wr * step_i + wi * step_r);
            }
        }
        size *= 2;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn tone(frequency: f32, amplitude: f32, invert_right: bool) -> [f32; 7] {
        let mut analyzer = Spectrum::new();
        for i in 0..FFT_SIZE {
            let sample = amplitude * (std::f32::consts::TAU * frequency * i as f32 / SAMPLE_RATE as f32).sin();
            analyzer.push(sample, if invert_right { -sample } else { sample });
        }
        analyzer.bands()
    }
    #[test]
    fn frequency_sweep_moves_tallest_band_from_bass_to_treble() {
        for (band, frequency) in [70., 160., 400., 1000., 2500., 6500., 14000.].into_iter().enumerate() {
            let levels = tone(frequency, 0.5, false);
            let strongest = levels.iter().enumerate().max_by(|a,b| a.1.total_cmp(b.1)).unwrap().0;
            assert_eq!(strongest, band, "frequency {frequency}: {levels:?}");
            assert!(levels[band] > 0.3 && levels[band] < 0.4, "RMS calibration: {levels:?}");
        }
    }
    #[test]
    fn stereo_phase_cannot_cancel_the_spectrum_and_volume_is_preserved() {
        let normal = tone(1000., 0.5, false);
        let inverted = tone(1000., 0.5, true);
        let quiet = tone(1000., 0.05, false);
        assert!(normal[3] > 0.3);
        assert!((normal[3] - inverted[3]).abs() < 0.00001);
        assert!((normal[3] / quiet[3] - 10.0).abs() < 0.001);
    }
    #[test]
    fn silence_is_zero_and_old_audio_is_overwritten() {
        let mut analyzer = Spectrum::new();
        assert_eq!(analyzer.bands(), [0.; 7]);
        for _ in 0..FFT_SIZE { analyzer.push(0.5, -0.5); }
        for _ in 0..FFT_SIZE { analyzer.push(0., 0.); }
        assert_eq!(analyzer.bands(), [0.; 7]);
    }
    #[test]
    fn mixed_frequencies_are_independent() {
        let mut analyzer = Spectrum::new();
        for i in 0..FFT_SIZE {
            let t = std::f32::consts::TAU * i as f32 / SAMPLE_RATE as f32;
            let sample = 0.4 * (160. * t).sin() + 0.1 * (6500. * t).sin();
            analyzer.push(sample, sample);
        }
        let bands = analyzer.bands();
        assert!(bands[1] > bands[5] * 3.5);
        assert!(bands[5] > 0.06);
        assert!(bands[3] < 0.001);
    }
}
