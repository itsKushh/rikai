use super::*;

impl Desk {
    pub(super) fn apply_inventory(&mut self, installed: Value, storage: Value) -> Result<()> {
        self.installed = installed;
        self.storage = storage;
        let prior = self.settings.clone();
        if let Some(redirects) = self.storage["redirects"].as_object() {
            for key in [
                "whisper",
                "gguf",
                "translation",
                "segmentation",
                "embedding",
            ] {
                if let Some(path) = self.settings[key].as_str() {
                    for (source, dest) in redirects {
                        if let Some(root) = dest.as_str()
                            && let Ok(rel) = std::path::Path::new(path)
                                .strip_prefix(PathBuf::from(source).join("modeles"))
                        {
                            let file = PathBuf::from(root).join("modeles").join(rel);
                            if file.exists() {
                                self.settings[key] = json!(file);
                                break;
                            }
                        }
                    }
                }
            }
        }
        for (key, name) in [
            ("gguf", "Résumé contextualisé"),
            ("gguf", "Résumé local"),
            ("translation", "Traduction locale"),
        ] {
            let selected = self.settings[key]
                .as_str()
                .is_some_and(|p| std::path::Path::new(p).exists());
            if !selected && self.available(name) {
                self.settings[key] = self.installed[name]["path"].clone();
            }
        }
        if self.available("Identification des voix") {
            for key in ["segmentation", "embedding"] {
                if self.settings[key]
                    .as_str()
                    .is_none_or(|path| !std::path::Path::new(path).is_file())
                {
                    self.settings[key] = self.installed["Identification des voix"][key].clone();
                }
            }
        }
        if prior != self.settings {
            self.save_settings()?;
        }
        Ok(())
    }

    pub(super) fn pick_storage(&mut self, w: &Window, cx: &mut Context<Self>) -> Result<()> {
        let dialog = rfd::AsyncFileDialog::new()
            .set_parent(w)
            .set_title(tr("Choisir un emplacement"))
            .set_directory(
                self.storage["roots"][0]
                    .as_str()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| self.root.clone()),
            );
        cx.spawn(async move |desk, cx| {
            let Some(chosen) = dialog.pick_folder().await else {
                return;
            };
            let _ = desk.update(cx, |s, cx| {
                if s.busy() {
                    return;
                }
                s.menu = None;
                s.page = Page::StorageChange;
                let tx = s.tx.clone();
                let ctl = s.begin_work();
                s.stage = "Emplacement des composants".into();
                let params = json!({
                    "destination": chosen.path(),
                    "transfer": s.settings["move_packages"] == true
                });
                thread::spawn(move || {
                    let result = crate::worker::run(0, "change_location", params, ctl, |event| {
                        let _ = tx.send(Event::Stage(event));
                    });
                    let _ = tx.send(Event::StorageChanged(result));
                });
                cx.notify();
            });
        })
        .detach();
        Ok(())
    }

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
        if entry["kind"] == "voices" {
            return live::voice_models_ready(entry);
        }
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
        self.page = Page::ConfirmDownload("Résumé contextualisé");
        Ok(())
    }
    pub(super) fn start_translation(&mut self, id: i64, language: String) -> Result<()> {
        ensure!(!self.busy(), "Un traitement est en cours");
        self.language_picker = None;
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
        let index = self
            .segments
            .iter()
            .position(|s| s.id == id)
            .context("Passage introuvable")?;
        let seg = &self.segments[index];
        let text = seg.text.clone();
        let context = self.segments[index.saturating_sub(2)..(index + 3).min(self.segments.len())]
            .iter()
            .filter(|s| s.track == seg.track && s.speaker == seg.speaker)
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let params = json!({"text":text,"context":context,"target":language,"source":self.db.segment_language(id)?,"folder":folder});
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
            self.begin_download()
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
            self.finish_download();
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
            "current" => {
                self.update_info = None;
                self.update_ready = false;
            }
            _ => {}
        }
        Ok(())
    }
}
