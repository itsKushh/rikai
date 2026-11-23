#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    use audio_capture::*;
    use std::{
        io::{self, BufRead},
        path::Path,
        sync::mpsc,
        thread,
        time::Duration,
    };
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd] if cmd == "sources" => println!("{}", serde_json::to_string_pretty(&discover()?)?),
        [cmd, output, selection] if cmd == "enregistrer" => {
            let inputs: Vec<Input> = serde_json::from_reader(std::fs::File::open(selection)?)?;
            let control = Control::default();
            let ctl = control.clone();
            let dir = Path::new(output).to_owned();
            let count = inputs.len();
            let worker = thread::spawn(move || record(&dir, inputs, ctl));
            let (tx, rx) = mpsc::channel();
            thread::spawn(move || {
                for s in io::stdin().lock().lines().map_while(Result::ok) {
                    if tx.send(s).is_err() {
                        break;
                    }
                }
            });
            let mut started = false;
            while !worker.is_finished() {
                if !started && control.ready() == count {
                    println!("REC");
                    started = true;
                }
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(s) if s.trim() == "pause" => control.pause(),
                    Ok(s) if s.trim() == "reprendre" => control.resume(),
                    Ok(s) if s.trim() == "stop" => control.stop(),
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        control.stop();
                        break;
                    }
                    _ => {}
                }
            }
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("Capture interrompue"))??;
        }
        _ => anyhow::bail!(
            "Usage : rikai-audio sources | enregistrer DOSSIER_NOUVEAU SÉLECTION.json (stdin : pause, reprendre, stop)"
        ),
    }
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("La capture par application nécessite Windows build 20348 ou ultérieur.");
    std::process::exit(1);
}
