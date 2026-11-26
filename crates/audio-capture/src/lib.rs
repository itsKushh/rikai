//! Pistes mono sur une grille commune de 16 kHz.
#[cfg(any(windows, test))]
mod microphone;
mod recovery;
use anyhow::{Context, Result, ensure};
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
pub use recovery::recover_tracks;
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::BufWriter,
    path::{Path, PathBuf},
};

#[cfg(windows)]
mod native;
#[cfg(windows)]
pub use native::*;

pub const RATE: u32 = 16_000;
pub const MIN_GAIN_DB: i8 = -18;
pub const MAX_GAIN_DB: i8 = 24;

#[cfg(any(windows, test))]
fn apply_gain(bytes: &mut [u8], gain: f32) -> f32 {
    let mut peak = 0u16;
    for pair in bytes.as_chunks_mut::<2>().0 {
        let mut value = i16::from_le_bytes(*pair);
        if gain != 1.0 {
            // La conversion saturante évite tout repliement à pleine échelle.
            value = (f32::from(value) * gain).round() as i16;
            *pair = value.to_le_bytes();
        }
        peak = peak.max(value.unsigned_abs());
    }
    f32::from(peak) / 32768.
}
pub const TICKS: u64 = 10_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Source {
    Application { pid: u32, born: u64, exe: PathBuf },
    Microphone { endpoint: String },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Input {
    pub name: String,
    pub source: Source,
    pub peak: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub file: String,
    pub input: Input,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub rate: u32,
    pub tracks: Vec<Track>,
}

#[derive(Clone)]
pub struct Timeline {
    start: u64,
    pauses: Vec<(u64, Option<u64>)>,
    stop: Option<u64>,
}
impl Timeline {
    pub fn new(start: u64) -> Self {
        Self {
            start,
            pauses: vec![],
            stop: None,
        }
    }
    pub fn pause(&mut self, at: u64) {
        if !self.paused() {
            self.pauses.push((at, None));
        }
    }
    pub fn resume(&mut self, at: u64) {
        if let Some((_, end @ None)) = self.pauses.last_mut() {
            *end = Some(at);
        }
    }
    pub fn stop(&mut self, at: u64) {
        self.stop.get_or_insert(at);
    }
    pub fn paused(&self) -> bool {
        self.pauses.last().is_some_and(|p| p.1.is_none())
    }
    pub fn frames(&self, at: u64) -> u64 {
        let at = at.min(self.stop.unwrap_or(at));
        let paused = self
            .pauses
            .iter()
            .map(|&(p, q)| at.min(q.unwrap_or(at)).saturating_sub(p))
            .sum::<u64>();
        (at.saturating_sub(self.start).saturating_sub(paused) as u128 * RATE as u128
            / TICKS as u128) as u64
    }
    fn recording(&self, at: u64) -> bool {
        at >= self.start
            && self.stop.is_none_or(|s| at < s)
            && !self
                .pauses
                .iter()
                .any(|&(p, q)| at >= p && q.is_none_or(|q| at < q))
    }
}
fn spec() -> WavSpec {
    WavSpec {
        channels: 1,
        sample_rate: RATE,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    }
}

/// Exporte exactement l’intervalle demandé en PCM, avec publication atomique.
pub fn export_segment(source: &Path, destination: &Path, start_ms: i64, end_ms: i64) -> Result<()> {
    ensure!(
        start_ms >= 0 && end_ms > start_ms,
        "Durée de passage invalide"
    );
    if destination.exists() {
        ensure!(
            source.canonicalize()? != destination.canonicalize()?,
            "Choisissez un autre fichier que la piste source."
        );
    }
    let mut input = WavReader::open(source)?;
    ensure!(input.spec() == spec(), "Format de piste incompatible");
    let start = (start_ms as u64).saturating_mul(u64::from(RATE)) / 1000;
    let end =
        ((end_ms as u64).saturating_mul(u64::from(RATE)) / 1000).min(u64::from(input.duration()));
    ensure!(start < end, "Passage en dehors de la piste");
    input.seek(start as u32)?;
    let mut tmp = tempfile::NamedTempFile::new_in(
        destination
            .parent()
            .context("Dossier de destination absent")?,
    )?;
    let mut out = WavWriter::new(BufWriter::new(tmp.as_file_mut()), spec())?;
    for sample in input.samples::<i16>().take((end - start) as usize) {
        out.write_sample(sample?)?;
    }
    out.finalize()?;
    tmp.persist(destination)?;
    Ok(())
}

pub struct TrackWriter {
    output: WavWriter<BufWriter<File>>,
    frames: u64,
    checkpoint: u64,
    checkpoint_frames: u64,
}
impl TrackWriter {
    pub fn create(path: &Path) -> Result<Self> {
        Self::with_checkpoint(path, u64::from(RATE))
    }
    pub fn create_live(path: &Path) -> Result<Self> {
        Self::with_checkpoint(path, u64::from(RATE) * 80 / 1000)
    }
    fn with_checkpoint(path: &Path, checkpoint_frames: u64) -> Result<Self> {
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        Ok(Self {
            output: WavWriter::new(BufWriter::new(file), spec())?,
            frames: 0,
            checkpoint: 0,
            checkpoint_frames,
        })
    }
    fn sample(&mut self, sample: i16) -> Result<()> {
        ensure!(
            self.frames < (u32::MAX as u64 - 64) / 2,
            "Taille limite du fichier WAV atteinte"
        );
        self.output.write_sample(sample)?;
        self.frames += 1;
        Ok(())
    }
    fn silence_to(&mut self, at: u64) -> Result<()> {
        while self.frames < at {
            self.sample(0)?;
        }
        Ok(())
    }
    pub fn packet(&mut self, at: u64, bytes: &[u8], clock: &Timeline) -> Result<()> {
        // Recaler au QPC à chaque paquet. Un écart positif représente du silence,
        // un chevauchement correspond à des instants déjà écrits.
        for (i, pair) in bytes.chunks_exact(2).enumerate() {
            let tick = at + i as u64 * TICKS / RATE as u64;
            if !clock.recording(tick) {
                continue;
            }
            let frame = clock.frames(tick);
            if frame < self.frames {
                continue;
            }
            self.silence_to(frame)?;
            self.sample(i16::from_le_bytes([pair[0], pair[1]]))?;
        }
        if self.frames - self.checkpoint >= self.checkpoint_frames {
            self.commit_pending()?;
        }
        Ok(())
    }
    fn commit_pending(&mut self) -> Result<()> {
        if self.frames > self.checkpoint {
            self.output.flush()?;
            self.checkpoint = self.frames;
        }
        Ok(())
    }
    pub fn finish(mut self, frames: u64) -> Result<()> {
        self.silence_to(frames)?;
        self.output.finalize()?;
        Ok(())
    }
}
/// Mix applicatif, en mémoire constante. Le microphone n'entre jamais dans le mix.
pub fn mix_apps(dir: &Path, tracks: &[Track]) -> Result<Option<PathBuf>> {
    let mut readers = tracks
        .iter()
        .filter(|t| matches!(t.input.source, Source::Application { .. }))
        .map(|t| WavReader::open(dir.join(&t.file)))
        .collect::<Result<Vec<_>, _>>()?;
    if readers.is_empty() {
        return Ok(None);
    }
    ensure!(
        readers.iter().all(|r| r.spec() == spec()),
        "Formats de pistes incompatibles"
    );
    let n = readers.len() as i64;
    let dest = dir.join("applications.wav");
    let tmp = dir.join("applications.partial");
    let mut sink = WavWriter::new(BufWriter::new(File::create_new(&tmp)?), spec())?;
    let mut streams: Vec<_> = readers.iter_mut().map(|r| r.samples::<i16>()).collect();
    loop {
        let mut sum = 0i64;
        let mut found = false;
        for stream in &mut streams {
            if let Some(v) = stream.next() {
                found = true;
                sum += i64::from(v?);
            }
        }
        if !found {
            break;
        }
        sink.write_sample((sum / n) as i16)?;
    }
    sink.finalize()?;
    std::fs::rename(tmp, &dest).context("Finalisation du mix")?;
    Ok(Some(dest))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn segment_export_copies_only_selected_frames_and_preserves_source() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let source = dir.path().join("piste.wav");
        let out = dir.path().join("extrait.wav");
        let mut wav = WavWriter::create(&source, spec())?;
        for i in 0..1600 {
            wav.write_sample(i as i16)?;
        }
        wav.finalize()?;
        export_segment(&source, &out, 25, 75)?;
        let mut clip = WavReader::open(&out)?;
        assert_eq!(clip.spec(), spec());
        assert_eq!(
            clip.samples::<i16>().collect::<Result<Vec<_>, _>>()?,
            (400..1200).collect::<Vec<i16>>()
        );
        assert!(export_segment(&source, &source, 0, 10).is_err());
        assert!(export_segment(&source, &out, 110, 150).is_err());
        assert_eq!(WavReader::open(&out)?.duration(), 800);
        Ok(())
    }

    #[test]
    fn calibration_measures_the_adjusted_pcm_and_saturates() {
        let source = [-24000i16, 18000, 400, -400, 0];
        let original: Vec<u8> = source.into_iter().flat_map(i16::to_le_bytes).collect();
        let mut samples = original.clone();
        assert_eq!(apply_gain(&mut samples, 1.), 24000. / 32768.);
        assert_eq!(original, samples);
        assert_eq!(apply_gain(&mut samples, 2.), 1.);
        let decoded: Vec<_> = samples
            .as_chunks::<2>()
            .0
            .iter()
            .copied()
            .map(i16::from_le_bytes)
            .collect();
        assert_eq!(decoded, [i16::MIN, i16::MAX, 800, -800, 0]);
        let mut attenuated = original;
        assert_eq!(apply_gain(&mut attenuated, 0.5), 12000. / 32768.);
        assert_eq!(apply_gain(&mut [0; 16], 12.), 0.);
    }

    #[test]
    fn live_audio_is_readable_before_stop_and_keeps_the_tail() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("live.wav");
        let mut writer = TrackWriter::create_live(&path)?;
        let clock = Timeline::new(0);
        let period = RATE as usize * 80 / 1000;
        writer.packet(0, &vec![0; period * 2], &clock)?;
        assert_eq!(WavReader::open(&path)?.duration() as usize, period);
        writer.packet(TICKS * 80 / 1000, &7i16.to_le_bytes(), &clock)?;
        writer.commit_pending()?;
        assert_eq!(WavReader::open(&path)?.samples::<i16>().last().unwrap()?, 7);
        writer.finish(period as u64 + 1)
    }

    #[test]
    fn gaps_and_pause_preserve_samples_and_order() -> Result<()> {
        let path = std::env::temp_dir().join(format!("mn-test-{}.wav", std::process::id()));
        let mut w = TrackWriter::create(&path)?;
        let mut clock = Timeline::new(1000);
        clock.pause(1000 + 3 * 625);
        clock.resume(1000 + 5 * 625);
        let data: Vec<_> = [1i16, 2, 3, 4, 5]
            .into_iter()
            .flat_map(i16::to_le_bytes)
            .collect();
        w.packet(1625, &data, &clock)?;
        w.finish(5)?;
        let samples = WavReader::open(&path)?
            .samples::<i16>()
            .collect::<Result<Vec<_>, _>>()?;
        std::fs::remove_file(path)?;
        assert_eq!(samples, [0, 1, 2, 5, 0]);
        Ok(())
    }
}
