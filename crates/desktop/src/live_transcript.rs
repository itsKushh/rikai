use serde_json::Value;
use std::{collections::BTreeMap, fmt::Write};

/// Start frame, input track and speaker turn within the utterance.
pub(crate) type PhraseKey = (u64, usize, u64);

#[derive(Default)]
pub(crate) struct FinalTranslations {
    lines: BTreeMap<PhraseKey, (String, String, String, u64)>,
}

impl FinalTranslations {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn observe(&mut self, key: PhraseKey, source: &str, speaker: &str, event: &Value) {
        if event["final"] != true {
            return;
        }
        let translation = event["translated"].as_str().unwrap_or("").trim();
        if source.trim().is_empty() || translation.is_empty() {
            self.lines.remove(&key);
        } else {
            self.lines.insert(
                key,
                (
                    speaker.to_owned(),
                    translation.to_owned(),
                    source.to_owned(),
                    event["speech_end"].as_u64().unwrap_or(key.0).max(key.0 + 1),
                ),
            );
        }
    }

    pub fn relabel(&mut self, key: PhraseKey, speaker: &str) {
        if let Some((name, ..)) = self.lines.get_mut(&key) {
            speaker.clone_into(name);
        }
    }

    pub fn passages(&self, rate: u32) -> Vec<(i64, i64, String, String, String)> {
        let millis = |n: u64| (n.saturating_mul(1000) / u64::from(rate)) as i64;
        self.lines
            .iter()
            .map(|((frame, _, _), (speaker, text, source, end))| {
                (
                    millis(*frame),
                    millis(*end).max(millis(*frame) + 1),
                    speaker.clone(),
                    source.clone(),
                    text.clone(),
                )
            })
            .collect()
    }
    pub fn text(&self, sample_rate: u32) -> String {
        let mut output = String::new();
        for ((frame, _, _), (speaker, text, ..)) in &self.lines {
            let seconds = frame / u64::from(sample_rate);
            let _ = write!(
                output,
                "[{:02}:{:02}:{:02}] ",
                seconds / 3600,
                seconds / 60 % 60,
                seconds % 60
            );
            if !speaker.is_empty() {
                let _ = write!(output, "{speaker} — ");
            }
            let _ = writeln!(output, "{text}");
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exports_a_whole_session_with_final_words_and_latest_speakers() {
        let mut session = FinalTranslations::default();
        assert!(session.is_empty());
        for second in (0..180).rev() {
            session.observe(
                (16000 * second, 0, 0),
                "salut",
                "Micro",
                &json!({"final":true,"translated":format!("Hello {second}")}),
            );
        }
        session.relabel((0, 0, 0), "Alex");
        session.observe(
            (0, 0, 1),
            "merci",
            "",
            &json!({"final":true,"translated":"ありがとう"}),
        );
        session.observe(
            (0, 1, 0),
            "hola",
            "Inès",
            &json!({"final":true,"translated":"Salut"}),
        );
        session.observe(
            (0, 1, 0),
            "adiós",
            "Inès",
            &json!({"final":true,"translated":"À bientôt"}),
        );
        session.observe((32000, 0, 0), "", "Micro", &json!({"final":true}));
        session.observe(
            (5000000, 0, 0),
            "à suivre",
            "Micro",
            &json!({"final":false,"translated":"…"}),
        );
        let text = session.text(16000);
        assert_eq!(text.lines().count(), 181);
        assert!(text.starts_with(
            "[00:00:00] Alex — Hello 0\n[00:00:00] ありがとう\n[00:00:00] Inès — À bientôt\n"
        ));
        assert!(text.ends_with("[00:02:59] Micro — Hello 179\n"));
        assert!(!text.contains("[00:00:02]"));
    }
}
