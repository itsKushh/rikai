use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
fn command() -> Result<Command> {
    let installed = std::env::current_exe()?
        .parent()
        .context("Dossier d’installation absent")?
        .join("worker/meetingnotes-worker.exe");
    if installed.is_file() {
        return Ok(Command::new(installed));
    }
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ai-worker");
    let python = src.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    ensure!(
        python.is_file(),
        "Worker IA absent. Installez le composant IA ou synchronisez ai-worker avec uv."
    );
    let mut cmd = Command::new(python);
    cmd.arg(src.join("worker.py"));
    Ok(cmd)
}
#[derive(Debug)]
pub struct Progress {
    pub stage: String,
    pub bytes: Option<(u64, u64)>,
}
impl Progress {
    fn parse(event: &Value) -> Result<Self> {
        let bytes = match (event.get("received"), event.get("total")) {
            (None, None) => None,
            (Some(n), Some(t)) => {
                let received = n.as_u64().context("Compteur de téléchargement invalide")?;
                let total = t.as_u64().context("Taille du téléchargement invalide")?;
                ensure!(received <= total, "Taille de téléchargement dépassée");
                Some((received, total))
            }
            _ => bail!("Progression de téléchargement incomplète"),
        };
        Ok(Self {
            stage: event["stage"]
                .as_str()
                .context("Étape manquante")?
                .to_owned(),
            bytes,
        })
    }
}
pub fn run(
    id: i64,
    method: &str,
    params: Value,
    cancel: Arc<AtomicBool>,
    progress: impl Fn(Progress),
) -> Result<Value> {
    exchange(
        command()?,
        json!({"version":1,"id":id,"method":method,"params":params}),
        cancel,
        progress,
        Duration::from_secs(90),
        Duration::from_secs(30 * 60),
    )
}

fn exchange(
    mut cmd: Command,
    request: Value,
    cancel: Arc<AtomicBool>,
    progress: impl Fn(Progress),
    startup_limit: Duration,
    idle_limit: Duration,
) -> Result<Value> {
    let id = request["id"]
        .as_i64()
        .context("Identifiant de travail absent")?;
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdin = child
        .stdin
        .take()
        .context("Canal de commande indisponible")?;
    let stdout = child
        .stdout
        .take()
        .context("Canal de réponse indisponible")?;
    let (tx, rx) = mpsc::sync_channel(8);
    let write_errors = tx.clone();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    // Une requête de résumé peut dépasser la capacité du pipe. Le superviseur
    // doit pouvoir annuler même si le processus n’a encore lu aucun octet.
    let (commands, controls) = mpsc::channel::<()>();
    let writer = thread::spawn(move || -> std::io::Result<()> {
        if let Err(e) = writeln!(stdin, "{request}").and_then(|_| stdin.flush()) {
            let _ = write_errors.send(Err(std::io::Error::new(
                e.kind(),
                format!("Envoi de la demande au moteur IA impossible · {e}"),
            )));
            return Ok(());
        }
        drop(write_errors);
        if controls.recv().is_ok() {
            writeln!(stdin, "{}", json!({"id":id,"method":"cancel"}))?;
            stdin.flush()?;
        }
        Ok(())
    });
    let result = (|| {
        let mut last_event = Instant::now();
        let mut stage: Option<String> = None;
        let mut response_limit = startup_limit;
        let mut stop = None;
        loop {
            if cancel.load(Ordering::Acquire) {
                if stop.is_none() {
                    let _ = commands.send(());
                    stop = Some(Instant::now());
                }
                if stop.is_some_and(|t| t.elapsed() >= Duration::from_secs(1)) {
                    bail!("Traitement annulé");
                }
            }
            if last_event.elapsed() >= response_limit {
                if let Some(stage) = &stage {
                    bail!(
                        "Le moteur IA ne répond plus à l’étape « {stage} » ({} s). Relancez le traitement.",
                        response_limit.as_secs()
                    );
                }
                bail!(
                    "Le moteur IA n’a pas répondu au démarrage ({} s). Relancez le traitement.",
                    startup_limit.as_secs()
                );
            }
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(line) => {
                    let e: Value = serde_json::from_str(&line?)?;
                    ensure!(
                        e["version"] == 1 && e["id"] == id,
                        "Réponse IPC incompatible"
                    );
                    match e["event"].as_str() {
                        Some("progress") => {
                            let update = Progress::parse(&e)?;
                            response_limit = match e["phase"].as_str() {
                                Some("startup") => startup_limit,
                                Some("load") => idle_limit.min(Duration::from_secs(300)),
                                _ => idle_limit,
                            };
                            stage = Some(update.stage.clone());
                            last_event = Instant::now();
                            progress(update);
                        }
                        Some("result") => return Ok(e["result"].clone()),
                        Some("error") => {
                            bail!("{}", e["message"].as_str().unwrap_or("Erreur du worker"))
                        }
                        _ => bail!("Événement IPC inconnu"),
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if cancel.load(Ordering::Acquire) {
                        bail!("Traitement annulé");
                    }
                    if let Some(status) = child.try_wait()? {
                        bail!("Le moteur IA s’est fermé sans résultat ({status}).");
                    }
                    bail!("Le moteur IA a fermé son canal de réponse sans résultat.")
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    })();
    drop(rx);
    if result.is_err() {
        let _ = child.kill();
    }
    drop(commands);
    // Après une réponse terminale, aucune inférence ne devrait encore tourner.
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            bail!("Le worker IA ne s’est pas fermé après la réponse");
        }
        thread::sleep(Duration::from_millis(20));
    };
    // Un descendant peut encore avoir hérité d’un pipe. La fin du processus
    // observée ci-dessus suffit, sans attendre un EOF supplémentaire.
    drop(reader);
    drop(writer);
    let value = result?;
    ensure!(
        status.success(),
        "Le worker a échoué après l’envoi du résultat"
    );
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stalled_child(
        script: &str,
        request: Value,
        startup: Duration,
        idle: Duration,
        cancel: Arc<AtomicBool>,
    ) -> String {
        let mut cmd = Command::new(command().unwrap().get_program());
        cmd.args(["-c", script]);
        exchange(cmd, request, cancel, |_| {}, startup, idle)
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn stalled_startup_and_full_request_pipe_have_a_deadline() {
        let msg = stalled_child(
            "import time; time.sleep(30)",
            json!({"id": 7, "params": "x".repeat(1024*1024)}),
            Duration::from_millis(350),
            Duration::from_secs(10),
            Arc::default(),
        );
        assert!(msg.contains("au démarrage"), "{msg}");
    }

    #[test]
    fn stalled_inference_identifies_last_stage() {
        let msg = stalled_child(
            r#"import sys,time
sys.stdin.readline()
print('{"version":1,"id":7,"event":"progress","stage":"chargement"}', flush=True)
time.sleep(30)"#,
            json!({"id": 7}),
            Duration::from_secs(15),
            Duration::from_millis(350),
            Arc::default(),
        );
        assert!(msg.contains("chargement"), "{msg}");
    }

    #[test]
    fn cancellation_interrupts_an_unresponsive_child() {
        let msg = stalled_child(
            "import time; time.sleep(30)",
            json!({"id": 7}),
            Duration::from_secs(15),
            Duration::from_secs(15),
            Arc::new(AtomicBool::new(true)),
        );
        assert!(msg.contains("annulé"), "{msg}");
    }

    #[test]
    fn python_request_and_structured_error() -> Result<()> {
        let cancel = Arc::new(AtomicBool::new(false));
        assert_eq!(
            run(9, "ping", json!({}), cancel.clone(), |_| {})?,
            json!({"protocol":1})
        );
        let err = run(10, "inconnu", json!({}), cancel, |_| {}).unwrap_err();
        assert!(err.to_string().contains("Méthode inconnue"));
        Ok(())
    }
}
