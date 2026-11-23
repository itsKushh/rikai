#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Page {
    Home,
    New,
    Library,
    Detail,
    Profiles,
    Settings,
    ConfirmDownload(&'static str),
}

impl Page {
    pub(crate) const MODEL_PRESETS: [&'static str; 4] =
        ["Rapide", "Équilibré", "Haute qualité", "Précision Turbo"];

    pub(crate) fn answer_download(&mut self, approved: bool) -> Option<&'static str> {
        let Self::ConfirmDownload(preset) = *self else {
            return None;
        };
        *self = Self::Settings;
        approved.then_some(preset)
    }
}

#[cfg(test)]
mod tests {
    use super::Page;

    #[test]
    fn each_preset_requires_its_own_confirmation() {
        for preset in Page::MODEL_PRESETS {
            let mut page = Page::ConfirmDownload(preset);
            assert_eq!(page.answer_download(false), None);
            assert_eq!(page, Page::Settings);
            assert_eq!(page.answer_download(true), None);

            page = Page::ConfirmDownload(preset);
            assert_eq!(page.answer_download(true), Some(preset));
            assert_eq!(page, Page::Settings);
            assert_eq!(page.answer_download(true), None);
        }

        for mut page in [
            Page::Home,
            Page::New,
            Page::Library,
            Page::Detail,
            Page::Profiles,
        ] {
            let current = page;
            assert_eq!(page.answer_download(true), None);
            assert_eq!(page, current);
        }
    }
}
