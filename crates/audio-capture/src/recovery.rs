use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

/// Reconstitue une copie lisible à partir du dernier en-tête WAV validé de chaque piste.
/// Les fichiers originaux restent intacts. Un dossier de travail unique isole les tentatives.
pub fn recover_tracks(dir: &Path, cancel: &AtomicBool) -> Result<(usize, usize)> {
    let original: Manifest = serde_json::from_reader(File::open(dir.join("sources.json"))?)?;
    ensure!(
        original.version == 1 && original.rate == RATE,
        "Format de session inconnu"
    );
    let dest = dir.join("reprise");
    ensure!(!dest.exists(), "Une récupération existe déjà");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let tmp = dir.join(format!("reprise-{nonce}.part"));
    std::fs::create_dir(&tmp)?;
    let copy = (|| {
        let mut valid = Vec::new();
        let mut longest = 0;
        let total = original.tracks.len();
        for track in original.tracks {
            let path = Path::new(&track.file);
            ensure!(
                path.components().count() == 1
                    && matches!(
                        path.components().next(),
                        Some(std::path::Component::Normal(_))
                    ),
                "Nom de piste invalide"
            );
            let Ok(mut read) = WavReader::open(dir.join(path)) else {
                continue;
            };
            if read.spec() != spec() || read.duration() == 0 {
                continue;
            }
            let mut out = WavWriter::create(tmp.join(path), spec())?;
            let mut n = 0u64;
            for sample in read.samples::<i16>() {
                if n % 16000 == 0 {
                    ensure!(!cancel.load(Ordering::Acquire), "Récupération annulée");
                }
                let Ok(sample) = sample else { break };
                out.write_sample(sample)?;
                n += 1;
            }
            out.finalize()?;
            if n > 0 {
                longest = longest.max(n);
                valid.push(track);
            }
        }
        ensure!(!valid.is_empty(), "Aucun fragment PCM lisible");
        ensure!(!cancel.load(Ordering::Acquire), "Récupération annulée");
        mix_apps(&tmp, &valid)?;
        let count = valid.len();
        let manifest = Manifest {
            version: 1,
            rate: RATE,
            tracks: valid,
        };
        std::fs::write(
            tmp.join("sources.json"),
            serde_json::to_vec_pretty(&manifest)?,
        )?;
        std::fs::write(
            tmp.join("finalise"),
            (longest * 1000 / u64::from(RATE)).to_string(),
        )?;
        ensure!(!cancel.load(Ordering::Acquire), "Récupération annulée");
        std::fs::rename(&tmp, &dest)?;
        Ok((count, total))
    })();
    if copy.is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    copy
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restore_skips_missing_track_and_excludes_microphone_from_mix() -> Result<()> {
        let root = std::env::temp_dir().join(format!("mn-reprise-{}", std::process::id()));
        std::fs::create_dir(&root)?;
        let mut tracks = vec![];
        for (name, source, value) in [
            (
                "a.wav",
                Source::Application {
                    pid: 5,
                    born: 7,
                    exe: "app.exe".into(),
                },
                40i16,
            ),
            (
                "b.wav",
                Source::Microphone {
                    endpoint: "micro".into(),
                },
                500,
            ),
        ] {
            let mut out = WavWriter::create(root.join(name), spec())?;
            for _ in 0..4096 {
                out.write_sample(value)?;
            }
            out.finalize()?;
            tracks.push(Track {
                file: name.into(),
                input: Input {
                    name: name.into(),
                    source,
                    peak: 0.,
                },
            });
        }
        let mut missing = tracks[0].clone();
        missing.file = "absente.wav".into();
        tracks.push(missing);
        std::fs::write(
            root.join("sources.json"),
            serde_json::to_vec(&Manifest {
                version: 1,
                rate: RATE,
                tracks,
            })?,
        )?;
        assert_eq!(recover_tracks(&root, &AtomicBool::new(false))?, (2, 3));
        let mixed = WavReader::open(root.join("reprise/applications.wav"))?
            .into_samples::<i16>()
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(mixed, vec![40; 4096]);
        assert!(root.join("b.wav").exists());
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
