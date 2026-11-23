"""Recette WASAPI sur Windows avec trois lecteurs audio et une vraie sortie active."""

import argparse
import cmath
import json
import math
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import time
import wave

TONES = (440, 997, 1531)


def player(hz, directory):
    path = directory / f"{hz}.wav"
    with wave.open(str(path), "wb") as wav:
        wav.setparams((1, 2, 48000, 0, "NONE", ""))
        wav.writeframes(
            b"".join(
                struct.pack("<h", round(3000 * math.sin(2 * math.pi * hz * i / 48000)))
                for i in range(48000)
            )
        )
    # Les trois lecteurs PowerShell sont frères, enfants de Python. Un chemin avec
    # des espaces reste une valeur de variable et ne devient pas du code PowerShell.
    return subprocess.Popen(
        [
            "powershell.exe",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$p = New-Object System.Media.SoundPlayer; $p.SoundLocation = $env:MN_TONE; $p.Load(); $p.PlayLooping(); "
            "while (-not (Test-Path -LiteralPath $env:MN_STOP)) { Start-Sleep -Milliseconds 100 }; $p.Stop()",
        ],
        env={**os.environ, "MN_TONE": str(path), "MN_STOP": str(directory / "stop")},
    )


def check_tones(path, included):
    with wave.open(str(path), "rb") as wav:
        assert wav.getframerate() == 16000 and wav.getnchannels() == 1
        assert wav.getnframes() >= 32000, "Capture trop courte"
        wav.setpos(wav.getnframes() // 2)
        data = struct.unpack("<16000h", wav.readframes(16000))
    energy = {
        hz: abs(
            sum(
                x * cmath.exp(-2j * math.pi * hz * i / 16000)
                for i, x in enumerate(data)
            )
        )
        / len(data)
        for hz in TONES
    }
    signal = min(energy[hz] for hz in included)
    noise = max(energy[hz] for hz in TONES if hz not in included)
    if signal < 15 or signal < 100 * max(noise, 0.001):
        raise AssertionError(f"Isolation insuffisante pour {path.name} : {energy}")
    print(
        f"{path.name}, fréquences attendues {included} Hz : {energy}, rejet supérieur à 40 dB"
    )


def validate(binary):
    with tempfile.TemporaryDirectory(prefix="test audio ") as tmp:
        root = Path(tmp)
        players = []
        try:
            for hz in TONES:
                players.append(player(hz, root))
            for _ in range(40):
                inputs = json.loads(
                    subprocess.check_output(
                        [binary, "sources"], encoding="utf-8", timeout=15
                    )
                )
                apps = {i["source"].get("pid"): i for i in inputs}
                if all(p.pid in apps for p in players):
                    break
                time.sleep(0.25)
            assert all(p.pid in apps for p in players), (
                "Trois sessions audio réelles requises"
            )
            for index, chosen in enumerate(((0,), (1,), (0, 1))):
                selection = root / "selection.json"
                out = root / f"capture-{index}"
                selection.write_text(
                    json.dumps([apps[players[i].pid] for i in chosen]), encoding="utf-8"
                )
                with subprocess.Popen(
                    [binary, "enregistrer", out, selection],
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    text=True,
                    encoding="utf-8",
                ) as recorder:
                    try:
                        time.sleep(6)
                        stdout, _ = recorder.communicate("stop\n", timeout=20)
                    finally:
                        if recorder.poll() is None:
                            recorder.kill()
                    assert recorder.returncode == 0 and "REC" in stdout, (
                        "Session non démarrée"
                    )
                check_tones(out / "applications.wav", [TONES[i] for i in chosen])
                for track, i in enumerate(chosen, 1):
                    check_tones(out / f"piste-{track}.wav", [TONES[i]])
        finally:
            (root / "stop").touch()
            for proc in players:
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    args = parser.parse_args()
    if sys.platform != "win32":
        parser.error("Une session Windows avec sortie audio est requise")
    validate(args.binary.resolve())
