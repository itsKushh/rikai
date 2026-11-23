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
pub fn run(
    id: i64,
    method: &str,
    params: Value,
    cancel: Arc<AtomicBool>,
    progress: impl Fn(String),
) -> Result<Value> {
    let mut cmd = command()?;
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
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let result = (|| {
        writeln!(
            stdin,
            "{}",
            json!({"version":1,"id":id,"method":method,"params":params})
        )?;
        stdin.flush()?;
        let mut stop = None;
        loop {
            if cancel.load(Ordering::Acquire) {
                if stop.is_none() {
                    let _ = writeln!(stdin, "{}", json!({"id":id,"method":"cancel"}));
                    stop = Some(Instant::now());
                }
                if stop.is_some_and(|t| t.elapsed() > Duration::from_secs(1)) {
                    bail!("Traitement annulé");
                }
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
                            progress(e["stage"].as_str().context("Étape manquante")?.to_owned())
                        }
                        Some("result") => return Ok(e["result"].clone()),
                        Some("error") => {
                            bail!("{}", e["message"].as_str().unwrap_or("Erreur du worker"))
                        }
                        _ => bail!("Événement IPC inconnu"),
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    bail!("Le worker s’est fermé sans résultat")
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    })();
    drop(rx);
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    drop(stdin);
    let _ = reader.join();
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
