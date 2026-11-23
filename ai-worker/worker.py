"""Un travail par processus. stdin reste ouvert pour cancel/shutdown."""

import json
import os
import sys
import threading

from models import capabilities, install
from voices import enroll
from processing import summarize, transcribe

os.environ["HF_HUB_DISABLE_TELEMETRY"] = "1"

sys.stdin.reconfigure(encoding="utf-8")
sys.stdout.reconfigure(encoding="utf-8")
# Réserver stdout du protocole ; les bibliothèques natives peuvent écrire sur le fd 1.
protocol = os.fdopen(os.dup(sys.stdout.fileno()), "w", encoding="utf-8", buffering=1)
os.dup2(sys.stderr.fileno(), sys.stdout.fileno())
out_lock = threading.Lock()


def emit(job, event, **values):
    with out_lock:
        print(
            json.dumps(dict(version=1, id=job, event=event, **values), allow_nan=False),
            file=protocol,
            flush=True,
        )


def selftest(params, progress):
    import ctranslate2
    import faster_whisper
    import sherpa_onnx
    import llama_cpp
    from faster_whisper.vad import get_speech_timestamps
    import numpy as np

    if get_speech_timestamps(np.zeros(16000, dtype=np.float32)):
        raise ValueError("Test de silence en échec")
    return dict(
        transcription=faster_whisper.__version__,
        diarisation=sherpa_onnx.__version__,
        ctranslate2=ctranslate2.__version__,
        llama=llama_cpp.__version__,
    )


def main():
    job = None
    try:
        req = json.loads(sys.stdin.readline())
        job = req["id"]
        if req["method"] != "install":
            os.environ["HF_HUB_OFFLINE"] = "1"
        if req["version"] != 1:
            raise ValueError("Version IPC incompatible")

        def controls():
            for line in sys.stdin:
                try:
                    command = json.loads(line)
                    if command.get("id") == job and command.get("method") in (
                        "cancel",
                        "shutdown",
                    ):
                        emit(job, "error", code="annule", message="Traitement annulé")
                        os._exit(2)
                except ValueError:
                    continue
            # Fin du canal de contrôle : le parent a quitté. Ne pas poursuivre une inférence orpheline.
            os._exit(2)

        # os.read/readline en arrière-plan ne doit pas bloquer la fermeture de l'interpréteur.
        if req["method"] in ("transcribe", "summarize", "enroll", "install"):
            threading.Thread(target=controls, daemon=True).start()
        engines = {
            "selftest": selftest,
            "install": install,
            "capabilities": capabilities,
            "enroll": enroll,
            "transcribe": transcribe,
            "summarize": summarize,
            "ping": lambda p, cb: {"protocol": 1},
            "shutdown": lambda p, cb: {},
        }
        if req["method"] not in engines:
            raise ValueError("Méthode inconnue")
        if req["method"] == "transcribe":
            import onnxruntime

            onnxruntime.disable_telemetry_events()
        result = engines[req["method"]](
            req.get("params", {}), lambda stage: emit(job, "progress", stage=stage)
        )
        emit(job, "result", result=result)
        return 0
    except Exception as exc:
        emit(job, "error", code=type(exc).__name__, message=str(exc))
        return 1


if __name__ == "__main__":
    os._exit(main())
