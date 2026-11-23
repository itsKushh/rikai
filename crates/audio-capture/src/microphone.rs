//! Décodage du format de mixage de l’entrée et conversion continue vers la grille des pistes.
use super::{RATE, TICKS};
use anyhow::{Result, anyhow, ensure};
use speexdsp_resampler::State;

pub(super) struct Microphone {
    rate: u32,
    channels: usize,
    bytes: usize,
    float: bool,
    filter: Option<State>,
    mono: Vec<f32>,
    filtered: Vec<f32>,
    output: Vec<u8>,
    next: Option<u64>,
    next_device_frame: Option<u64>,
}
impl Microphone {
    pub fn new(rate: u32, channels: usize, bits: u16, float: bool) -> Result<Self> {
        ensure!(
            (8000..=384000).contains(&rate) && (1..=32).contains(&channels),
            "Format microphone non pris en charge ({rate} Hz, {channels} canaux)"
        );
        ensure!(
            if float {
                bits == 32 || bits == 64
            } else {
                matches!(bits, 8 | 16 | 24 | 32)
            },
            "Précision micro non prise en charge ({bits} bits)"
        );
        let filter = if rate == RATE {
            None
        } else {
            Some(
                State::new(1, rate as usize, RATE as usize, 7)
                    .map_err(|e| anyhow!("Conversion micro · {e:?}"))?,
            )
        };
        Ok(Self {
            rate,
            channels,
            bytes: usize::from(bits / 8),
            float,
            filter,
            mono: vec![],
            filtered: vec![],
            output: vec![],
            next: None,
            next_device_frame: None,
        })
    }
    fn sample(&self, data: &[u8]) -> f32 {
        match (self.float, self.bytes) {
            (true, 4) => f32::from_le_bytes(data.try_into().unwrap()),
            (true, 8) => f64::from_le_bytes(data.try_into().unwrap()) as f32,
            (_, 1) => (f32::from(data[0]) - 128.) / 128.,
            (_, 2) => f32::from(i16::from_le_bytes(data.try_into().unwrap())) / 32768.,
            (_, 3) => i32::from_le_bytes([0, data[0], data[1], data[2]]) as f32 / 2147483648.,
            (_, 4) => i32::from_le_bytes(data.try_into().unwrap()) as f32 / 2147483648.,
            _ => unreachable!(),
        }
    }
    pub fn packet(
        &mut self,
        data: &[u8],
        observed: u64,
        device_frame: u64,
        discontinuity: bool,
    ) -> Result<(u64, &mut [u8])> {
        let stride = self.channels * self.bytes;
        ensure!(data.len().is_multiple_of(stride), "Paquet micro incomplet");
        self.mono.clear();
        for frame in data.chunks_exact(stride) {
            let sample = frame
                .chunks_exact(self.bytes)
                .map(|v| self.sample(v))
                .sum::<f32>()
                / self.channels as f32;
            self.mono.push(if sample.is_finite() { sample } else { 0. });
        }
        // Un tampon virtuel peut être livré avec quelques ms de gigue QPC. Les échantillons
        // consécutifs gardent leur cadence, y compris de part et d’autre d’un paquet WASAPI.
        let missing = self
            .next_device_frame
            .and_then(|n| device_frame.checked_sub(n))
            .unwrap_or(0);
        self.next_device_frame = Some(device_frame.saturating_add(self.mono.len() as u64));
        let continuous = self.next.filter(|t| {
            observed.abs_diff(*t) < TICKS / 25
                && !(discontinuity && (missing > 0 || observed.abs_diff(*t) > TICKS / 200))
        });
        let start = match (continuous, self.next) {
            (Some(t), _) => t,
            (None, Some(t))
                if discontinuity
                    && missing > 0
                    && missing < u64::from(self.rate)
                    && observed.abs_diff(t) < TICKS / 200 =>
            {
                t + missing * TICKS / u64::from(self.rate)
            }
            _ => observed,
        };
        let samples = if let Some(filter) = &mut self.filter {
            if continuous.is_none() && self.next.is_some() {
                filter.reset();
            }
            self.filtered.resize(
                (self.mono.len() * RATE as usize).div_ceil(self.rate as usize) + 2,
                0.,
            );
            let (read, written) = filter
                .process_float(0, &self.mono, &mut self.filtered)
                .map_err(|e| anyhow!("Rééchantillonnage du microphone · {e:?}"))?;
            ensure!(
                read == self.mono.len(),
                "Conversion incomplète du microphone"
            );
            &self.filtered[..written]
        } else {
            &self.mono[..]
        };
        self.output.clear();
        self.output.extend(
            samples
                .iter()
                .flat_map(|s| ((s * 32768.).round() as i16).to_le_bytes()),
        );
        self.next = Some(start + samples.len() as u64 * TICKS / u64::from(RATE));
        Ok((start, &mut self.output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packed_pcm_and_device_discontinuity() -> Result<()> {
        let mut mic = Microphone::new(16000, 2, 24, false)?;
        let stereo = [0, 0, 0x40, 0, 0, 0x20, 0, 0, 0xc0, 0, 0, 0xe0];
        let (at, pcm) = mic.packet(&stereo, 10000, 20, false)?;
        assert_eq!(at, 10000);
        assert_eq!(pcm, [0, 48, 0, 208]);
        let (at, _) = mic.packet(&stereo, 11250, 38, true)?;
        assert_eq!(at, 11250 + 16 * 625);
        Ok(())
    }
    #[test]
    fn stereo_virtual_microphone_preserves_voice_across_jittery_packets() -> Result<()> {
        for rate in [44100, 48000] {
            let mut mic = Microphone::new(rate, 2, 32, true)?;
            let mut output = Vec::new();
            let mut start = 0;
            while start < rate {
                let n = (137 + start % 271).min(rate - start);
                let mut bytes = vec![];
                for t in start..start + n {
                    let s = 0.4 * (2. * std::f32::consts::PI * 440. * t as f32 / rate as f32).sin();
                    bytes.extend(s.to_le_bytes());
                    bytes.extend(s.to_le_bytes());
                }
                let observed = u64::from(start) * TICKS / u64::from(rate)
                    + if start > 0 && start % 3 == 0 {
                        20000
                    } else {
                        0
                    };
                let (at, pcm) = mic.packet(&bytes, observed, u64::from(start), start % 2 == 0)?;
                assert_eq!(at, output.len() as u64 * TICKS / u64::from(RATE));
                output.extend(
                    pcm.chunks_exact(2)
                        .map(|v| i16::from_le_bytes([v[0], v[1]]) as f32),
                );
                start += n;
            }
            assert!(output.len().abs_diff(RATE as usize) <= 1);
            let middle = &output[300..15500];
            let energy = middle.iter().map(|s| s * s).sum::<f32>();
            let power = |phase: f32| {
                middle
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        s * (2. * std::f32::consts::PI * 440. * i as f32 / RATE as f32 + phase)
                            .sin()
                    })
                    .sum::<f32>()
                    .powi(2)
            };
            assert!(
                (power(0.) + power(std::f32::consts::FRAC_PI_2)) * 2.
                    / middle.len() as f32
                    / energy
                    > 0.998
            );
        }
        Ok(())
    }
}
