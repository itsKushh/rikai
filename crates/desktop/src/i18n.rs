use regex::Regex;
use std::{
    collections::BTreeMap,
    sync::{
        OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};

pub const LOCALES: [(&str, &str); 6] = [
    ("fr", "Français"),
    ("en", "English"),
    ("es", "Español"),
    ("ru", "Русский"),
    ("ja", "日本語"),
    ("zh", "中文"),
];
static CURRENT: AtomicUsize = AtomicUsize::new(0);
type Catalog = BTreeMap<String, [String; 5]>;
fn catalog() -> &'static Catalog {
    static DICT: OnceLock<Catalog> = OnceLock::new();
    DICT.get_or_init(|| {
        serde_json::from_str(include_str!("../../../assets/locales.json"))
            .expect("Rikai locale catalog")
    })
}
fn index() -> usize {
    CURRENT.load(Ordering::Relaxed)
}
pub fn code() -> &'static str {
    LOCALES[index()].0
}
pub fn select(requested: Option<&str>) {
    let os = system_locale();
    let code = requested
        .filter(|s| !s.is_empty())
        .unwrap_or(&os)
        .split(['-', '_', '.'])
        .next()
        .unwrap_or("en");
    CURRENT.store(
        LOCALES
            .iter()
            .position(|(c, _)| c.eq_ignore_ascii_case(code))
            .unwrap_or(1),
        Ordering::Relaxed,
    );
}
#[cfg(windows)]
fn system_locale() -> String {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetUserDefaultLocaleName(buf: *mut u16, len: i32) -> i32;
        fn GetUserPreferredUILanguages(
            flags: u32,
            count: *mut u32,
            buf: *mut u16,
            len: *mut u32,
        ) -> i32;
    }
    let (mut count, mut need) = (0u32, 0u32);
    unsafe {
        GetUserPreferredUILanguages(8, &mut count, std::ptr::null_mut(), &mut need);
    }
    if (2..=65536).contains(&need) {
        let mut names = vec![0u16; need as usize];
        if unsafe { GetUserPreferredUILanguages(8, &mut count, names.as_mut_ptr(), &mut need) } != 0
        {
            for name in names.split(|v| *v == 0).filter(|v| !v.is_empty()) {
                let tag = String::from_utf16_lossy(name);
                if LOCALES.iter().any(|(code, _)| {
                    tag.split('-')
                        .next()
                        .is_some_and(|s| s.eq_ignore_ascii_case(code))
                }) {
                    return tag;
                }
            }
            return "en".into();
        }
    }
    let mut buf = [0u16; 85];
    let len = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
    if len > 1 {
        String::from_utf16_lossy(&buf[..len as usize - 1])
    } else {
        "en".into()
    }
}
#[cfg(not(windows))]
fn system_locale() -> String {
    std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_else(|_| "en".into())
}
pub fn tr(key: &str) -> &str {
    let i = index();
    if i == 0 {
        return key;
    }
    catalog()
        .get(key)
        .map(|row| row[i - 1].as_str())
        .unwrap_or(key)
}
// Formatting is done by the caller. Capture the substituted values once and
// apply their translated ordering; never translate a user's transcript.
pub fn message(source: &str) -> String {
    let i = index();
    if i == 0 {
        return source.into();
    }
    if let Some(row) = catalog().get(source) {
        return row[i - 1].clone();
    }
    static FORMATS: OnceLock<Vec<(Regex, &'static [String; 5])>> = OnceLock::new();
    let formats = FORMATS.get_or_init(|| {
        let var = Regex::new(r"\{[^{}]*\}").unwrap();
        let mut formats = vec![];
        for (key, row) in catalog().iter().filter(|(k, _)| k.contains('{')) {
            let mut pattern = "(?s)^".to_string();
            let mut end = 0;
            for m in var.find_iter(key) {
                pattern.push_str(&regex::escape(&key[end..m.start()]));
                pattern.push_str("(.*?)");
                end = m.end();
            }
            pattern.push_str(&regex::escape(&key[end..]));
            pattern.push('$');
            formats.push((key.len(), Regex::new(&pattern).unwrap(), row));
        }
        formats.sort_by_key(|v| std::cmp::Reverse(v.0));
        formats.into_iter().map(|(_, r, v)| (r, v)).collect()
    });
    for (pattern, row) in formats {
        if let Some(values) = pattern.captures(source) {
            static SLOT: OnceLock<Regex> = OnceLock::new();
            return SLOT
                .get_or_init(|| Regex::new(r"\{([0-9]+)\}").unwrap())
                .replace_all(&row[i - 1], |c: &regex::Captures| {
                    let n: usize = c[1].parse().unwrap();
                    values
                        .get(n + 1)
                        .map(|m| m.as_str())
                        .unwrap_or("")
                        .to_owned()
                })
                .into_owned();
        }
    }
    source.into()
}
pub fn languages() -> &'static Vec<[String; 8]> {
    static NAMES: OnceLock<Vec<[String; 8]>> = OnceLock::new();
    NAMES.get_or_init(|| {
        serde_json::from_str(include_str!("../../../assets/translation-languages.json")).unwrap()
    })
}
pub fn language_name(code: &str) -> &str {
    languages()
        .iter()
        .find(|row| row[0] == code)
        .map(|row| row[index() + 2].as_str())
        .unwrap_or(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn six_interface_languages_and_independent_translation_targets() {
        let targets = languages();
        assert_eq!(targets.len(), 100);
        assert_eq!(
            targets
                .iter()
                .map(|r| &r[0])
                .collect::<std::collections::HashSet<_>>()
                .len(),
            100
        );
        for (language, _) in LOCALES {
            select(Some(language));
            assert_eq!(code(), language);
            assert!(!tr("Paramètres").is_empty());
            let message = message("Temps écoulé 05:23 · dernière réponse il y a 19 s");
            assert!(message.contains("05:23") && message.contains("19") && !message.contains('{'));
            assert!(!language_name("ja").is_empty());
        }
        select(Some("fr-CA"));
        assert_eq!(code(), "fr");
    }
}
