use super::*;
use std::fmt::Write;
pub const FORMATS: [(&str, &str, &str); 7] = [
    ("txt", "Texte simple", "txt"),
    ("speakers", "Texte avec intervenants", "txt"),
    ("timed", "Texte horodaté", "txt"),
    ("srt", "Sous-titres SRT", "srt"),
    ("vtt", "Sous-titres WebVTT", "vtt"),
    ("md", "Document Markdown", "md"),
    ("json", "Segments structurés", "json"),
];
pub fn stamp(ms: i64, sep: char) -> String {
    let ms = ms.max(0);
    format!(
        "{:02}:{:02}:{:02}{sep}{:03}",
        ms / 3600000,
        ms / 60000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}
pub fn transcript(title: &str, segments: &[Segment], format: &str) -> Result<String> {
    ensure!(!segments.is_empty(), "Aucune intervention transcrite.");
    let mut out = match format {
        "vtt" => "WEBVTT\n\n".into(),
        "md" => format!("# {title}\n\n"),
        "json" => {
            return Ok(serde_json::to_string_pretty(
                &serde_json::json!({"title":title,"segments":segments}),
            )?);
        }
        _ => String::new(),
    };
    for (i, s) in segments.iter().enumerate() {
        let text = s.text.split_whitespace().collect::<Vec<_>>().join(" ");
        let named = if s.speaker.is_empty() {
            text.clone()
        } else {
            format!("{} — {text}", s.speaker)
        };
        let sep = if format == "srt" { ',' } else { '.' };
        let a = stamp(s.start_ms, sep);
        let b = stamp(s.end_ms.max(s.start_ms + 1), sep);
        match format {
            "txt" => writeln!(out, "{text}")?,
            "speakers" => writeln!(out, "{named}")?,
            "timed" => writeln!(out, "[{a} – {b}] {named}")?,
            "md" => writeln!(out, "- **{a}** · {named}")?,
            "srt" | "vtt" => {
                if format == "srt" {
                    writeln!(out, "{}", i + 1)?
                }
                let plain = named
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                writeln!(out, "{a} --> {b}\n{plain}\n")?;
            }
            _ => anyhow::bail!("Format d’export inconnu"),
        }
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_and_caption_outputs() -> Result<()> {
        let lines = vec![serde_json::from_value(
            serde_json::json!({"start_ms":10,"end_ms":1304,"speaker":"Lou","text":"Ça va ?","track":"a.wav","words":[]}),
        )?];
        assert_eq!(transcript("A", &lines, "txt")?, "Ça va ?\n");
        assert_eq!(
            transcript("A", &lines, "srt")?,
            "1\n00:00:00,010 --> 00:00:01,304\nLou — Ça va ?\n\n"
        );
        assert!(transcript("A", &lines, "vtt")?.starts_with("WEBVTT\n\n00:00:00.010"));
        for (fmt, _, _) in FORMATS {
            assert!(!transcript("A", &lines, fmt)?.is_empty());
        }
        Ok(())
    }
}
