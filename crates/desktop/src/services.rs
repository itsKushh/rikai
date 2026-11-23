use super::*;

impl Desk {
    pub(super) fn repo_for(preset: &str) -> &'static str {
        match preset {
            "Rapide" => "Systran/faster-whisper-base",
            "Équilibré" => "Systran/faster-whisper-small",
            "Haute qualité" => "Systran/faster-whisper-medium",
            "Précision Turbo" => "mobiuslabsgmbh/faster-whisper-large-v3-turbo",
            _ => "",
        }
    }
    pub(super) fn available(&self, preset: &str) -> bool {
        let entry = &self.installed[preset];
        if let Some(path) = entry["path"].as_str() {
            return std::path::Path::new(path).exists();
        }
        entry["folder"]
            .as_str()
            .is_some_and(|p| std::path::Path::new(p).join("model.bin").is_file())
    }
    pub(super) fn select_model(&mut self, preset: &'static str) -> Result<()> {
        self.menu = None;
        if self.available(preset) {
            let entry = &self.installed[preset];
            self.settings["whisper"] = entry["folder"].clone();
            self.settings["whisper_repo"] = entry["repo"].clone();
            self.settings["whisper_revision"] = entry["revision"].clone();
            self.save_settings()?;
        } else {
            self.page = Page::ConfirmDownload(preset);
        }
        Ok(())
    }
    pub(super) fn offer_summary(&mut self) -> Result<()> {
        if self.settings["gguf"]
            .as_str()
            .is_some_and(|p| std::path::Path::new(p).is_file())
        {
            return self.process(true);
        }
        self.settings_origin = self.page;
        self.pending_summary = true;
        self.section = 0;
        self.page = Page::ConfirmDownload("Résumé local");
        Ok(())
    }
    pub(super) fn start_translation(&mut self, id: i64, language: String) -> Result<()> {
        ensure!(!self.busy(), "Un traitement est en cours");
        self.language_picker = None;
        if self
            .translations
            .get(&id)
            .is_some_and(|v| v.iter().any(|(l, _)| l == &language))
        {
            return Ok(());
        }
        let folder = self.settings["translation"]
            .as_str()
            .filter(|p| std::path::Path::new(p).join("model.bin").is_file());
        let Some(folder) = folder else {
            self.pending_translation = Some((id, language));
            self.settings_origin = self.page;
            self.section = 1;
            self.page = Page::ConfirmDownload("Traduction locale");
            return Ok(());
        };
        let text = self
            .segments
            .iter()
            .find(|s| s.id == id)
            .context("Passage introuvable")?
            .text
            .clone();
        let params = json!({"text":text,"target":language,"source":self.db.segment_language(id)?,"folder":folder});
        let cancel = self.begin_work();
        self.stage = "Traduction du passage".into();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = crate::worker::run(id, "translate", params, cancel, |v| {
                let _ = tx.send(Event::Stage(v));
            });
            let _ = tx.send(Event::Translated(id, text, language, result));
        });
        Ok(())
    }
    pub(super) fn update_request(&mut self, op: &'static str) {
        if self.update_checking
            || self.busy()
            || (op == "check_update"
                && self.settings["updates_auto"] == false
                && self.page != Page::Settings)
        {
            return;
        }
        let info = self.update_info.as_ref();
        let method = if op == "restart_update" || op == "close_update" {
            "apply_update"
        } else {
            op
        };
        let params = json!({"version":env!("CARGO_PKG_VERSION"),"locale":crate::i18n::code(),"target":info.map(|v|&v["version"]),"sha256":info.map(|v|&v["sha256"]),"restart":op=="restart_update"});
        self.update_checking = true;
        self.update_downloading = method == "download_update";
        let cancel = if method == "download_update" {
            self.transfer = None;
            self.stage = "Téléchargement de la mise à jour".into();
            self.begin_work()
        } else {
            Arc::new(AtomicBool::new(false))
        };
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = crate::worker::run(0, method, params, cancel, |v| {
                let _ = tx.send(Event::Stage(v));
            });
            let _ = tx.send(Event::Update(op, result));
        });
    }
    pub(super) fn finish_update(&mut self, op: &str, res: Result<Value>) -> Result<()> {
        self.update_checking = false;
        self.update_downloading = false;
        if op == "download_update" {
            self.cancel = None;
        }
        let data = match res {
            Ok(v) => v,
            Err(e) => {
                self.update_status = format!("{e:#}");
                return Ok(());
            }
        };
        self.update_status = data["state"].as_str().unwrap_or_default().to_owned();
        match self.update_status.as_str() {
            "available" | "ready" => {
                self.update_ready = self.update_status == "ready";
                self.update_info = Some(data);
            }
            "disconnected" | "current" => {
                self.update_info = None;
                self.update_ready = false;
            }
            _ => {}
        }
        Ok(())
    }
}
