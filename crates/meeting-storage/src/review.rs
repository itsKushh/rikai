//! Priorité de relecture fondée sur les indices disponibles dans la transcription.
use crate::Segment;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Reason {
    Negation,
    Quantity,
    Wording,
    Overlap,
    Speaker,
    Echo,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flag {
    pub reason: Reason,
    pub word: Option<usize>,
}

fn word_reason(text: &str) -> Reason {
    let token = text
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    const NEGATIONS: &[&str] = &[
        "pas", "jamais", "aucun", "aucune", "rien", "sans", "non", "not", "never", "no", "ningún",
        "nunca", "не", "нет",
    ];
    const QUANTITIES: &[&str] = &[
        "zéro",
        "deux",
        "trois",
        "quatre",
        "cinq",
        "six",
        "sept",
        "huit",
        "neuf",
        "dix",
        "onze",
        "douze",
        "treize",
        "quatorze",
        "quinze",
        "seize",
        "vingt",
        "vingts",
        "trente",
        "quarante",
        "cinquante",
        "soixante",
        "septante",
        "huitante",
        "nonante",
        "cent",
        "cents",
        "mille",
        "million",
        "millions",
        "milliard",
        "milliards",
        "euro",
        "euros",
        "dollar",
        "dollars",
    ];
    if NEGATIONS.contains(&token.as_str())
        || token.starts_with("n’")
        || token.starts_with("n'")
        || token.ends_with("n't")
        || token.ends_with("n’t")
    {
        Reason::Negation
    } else if text
        .chars()
        .any(|c| c.is_numeric() || matches!(c, '€' | '$' | '£' | '¥' | '%'))
        || token
            .split(['-', '‑'])
            .any(|part| QUANTITIES.contains(&part))
    {
        Reason::Quantity
    } else {
        Reason::Wording
    }
}

pub fn flag(segment: &Segment) -> Option<Flag> {
    if segment.reviewed || segment.text.trim().is_empty() {
        return None;
    }
    // Only give a word semantic priority when that word itself is uncertain.
    let weakest = segment
        .words
        .iter()
        .enumerate()
        .filter_map(|(i, w)| {
            let score = w.confidence?;
            (score < 0.60 && !w.text.trim().is_empty()).then(|| (word_reason(&w.text), score, i))
        })
        .min_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));
    if let Some((reason, _, i)) = weakest {
        return Some(Flag {
            reason,
            word: Some(i),
        });
    }
    let reason = if segment.confidence.is_some_and(|v| v < 0.65) {
        Reason::Wording
    } else if segment.acoustic_label.starts_with("Chevauchement ·") {
        Reason::Overlap
    } else if segment.speaker == "Intervenant incertain" {
        Reason::Speaker
    } else if segment.echo_candidate.is_some() {
        Reason::Echo
    } else {
        return None;
    };
    Some(Flag { reason, word: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Word;

    #[test]
    fn ranks_only_uncertain_words_and_respects_missing_evidence() {
        let mut s: Segment = serde_json::from_value(serde_json::json!({
            "start_ms": 0, "end_ms": 960, "track": "1", "speaker": "Voix distante",
            "text": "Euh, pas 50 euros.", "words": []
        }))
        .unwrap();
        assert_eq!(flag(&s), None);
        s.words = [("Euh,", 0.1), ("pas", 0.48), ("50", 0.27), ("euros.", 0.91)]
            .map(|(text, confidence)| Word {
                text: text.into(),
                start_ms: 0,
                end_ms: 200,
                confidence: Some(confidence),
            })
            .to_vec();
        assert_eq!(
            flag(&s),
            Some(Flag {
                reason: Reason::Negation,
                word: Some(1)
            })
        );
        s.words[1].confidence = Some(0.99);
        assert_eq!(
            flag(&s),
            Some(Flag {
                reason: Reason::Quantity,
                word: Some(2)
            })
        );
        s.words[2].confidence = Some(0.91);
        assert_eq!(
            flag(&s),
            Some(Flag {
                reason: Reason::Wording,
                word: Some(0)
            })
        );
        s.words.clear();
        s.speaker = "Intervenant incertain".into();
        assert_eq!(flag(&s).unwrap().reason, Reason::Speaker);
        s.reviewed = true;
        assert_eq!(flag(&s), None);
    }
}
