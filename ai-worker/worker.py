"""Un travail par processus. stdin reste ouvert pour cancel/shutdown."""

from contextlib import nullcontext
import json
import os
import sys
import threading

if __name__ == "__main__":
    from multiprocessing import freeze_support

    freeze_support()

from models import capabilities, install
from package_storage import change_location, storage_lock
from translation import translate
from live import listen
from updates import apply_update, check_update, download_update
from voices import enroll
from processing import gpu_probe, summarize, transcribe
from nvidia_runtime import install_runtime

os.environ["HF_HUB_DISABLE_TELEMETRY"] = "1"

sys.stdin.reconfigure(encoding="utf-8")
sys.stdout.reconfigure(encoding="utf-8")
# Réserver stdout du protocole ; les bibliothèques natives peuvent écrire sur le fd 1.
protocol = os.fdopen(os.dup(sys.stdout.fileno()), "w", encoding="utf-8", buffering=1)
os.dup2(sys.stderr.fileno(), sys.stdout.fileno())
out_lock = threading.Lock()
terminal_sent = threading.Event()


def emit(job, event, **values):
    with out_lock:
        if terminal_sent.is_set():
            return False
        print(
            json.dumps(dict(version=1, id=job, event=event, **values), allow_nan=False),
            file=protocol,
            flush=True,
        )
        if event in ("result", "error"):
            terminal_sent.set()
        return True


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
        llama_cpu=llama_cpp.llama_print_system_info().decode("utf-8"),
    )


def command_lines():
    # La lecture bloquante du CRT Windows garde le verrou du descripteur stdin.
    # Certaines DLL consultent les descripteurs pendant leur initialisation.
    if sys.platform != "win32":
        yield from sys.stdin
        return

    import ctypes
    import msvcrt
    from ctypes import wintypes

    handle = wintypes.HANDLE(msvcrt.get_osfhandle(sys.stdin.fileno()))
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.GetFileType.argtypes = [wintypes.HANDLE]
    kernel.GetFileType.restype = wintypes.DWORD
    if kernel.GetFileType(handle) != 3:  # FILE_TYPE_PIPE
        yield from sys.stdin
        return
    msvcrt.setmode(sys.stdin.fileno(), os.O_BINARY)
    peek = kernel.PeekNamedPipe
    peek.argtypes = [
        wintypes.HANDLE,
        wintypes.LPVOID,
        wintypes.DWORD,
        ctypes.POINTER(wintypes.DWORD),
        ctypes.POINTER(wintypes.DWORD),
        ctypes.POINTER(wintypes.DWORD),
    ]
    peek.restype = wintypes.BOOL
    available = wintypes.DWORD()
    pending = bytearray()
    while not terminal_sent.is_set():
        if not peek(handle, None, 0, None, ctypes.byref(available), None):
            error = ctypes.get_last_error()
            if error in (109, 232, 233):  # canal fermé ou déconnecté
                break
            raise ctypes.WinError(error)
        if not available.value:
            terminal_sent.wait(0.05)
            continue
        chunk = os.read(sys.stdin.fileno(), min(available.value, 65536))
        if not chunk:
            break
        lines = chunk.split(b"\n")
        pending.extend(lines[0])
        for tail in lines[1:]:
            yield bytes(pending)
            pending = bytearray(tail)
    if pending:
        yield bytes(pending)


def main():
    job = None
    reader = None
    try:
        commands = command_lines()
        req = json.loads(next(commands))
        job = req["id"]
        if req["version"] != 1:
            raise ValueError("Version IPC incompatible")
        method = req["method"]
        params = req.get("params", {})
        # The Hub snapshots this flag on import. Each worker handles a single
        # request, and only a confirmed model install needs an online Hub client.
        online_install = method == "install" and params.get("consent") is True
        os.environ["HF_HUB_OFFLINE"] = "0" if online_install else "1"

        def controls():
            for line in commands:
                try:
                    command = json.loads(line)
                    if command.get("id") == job and command.get("method") in (
                        "cancel",
                        "shutdown",
                    ):
                        if emit(
                            job, "error", code="annule", message="Traitement annulé"
                        ):
                            os._exit(2)
                        return
                except ValueError:
                    continue
            # Fin du canal de contrôle : le parent a quitté.
            with out_lock:
                if not terminal_sent.is_set():
                    os._exit(2)

        # os.read/readline en arrière-plan ne doit pas bloquer la fermeture de l'interpréteur.
        if req["method"] in (
            "transcribe",
            "summarize",
            "enroll",
            "install",
            "install_cuda",
            "test_cuda",
            "translate",
            "listen",
            "download_update",
        ):
            reader = threading.Thread(target=controls, daemon=True)
            reader.start()
        engines = {
            "apply_update": apply_update,
            "translate": translate,
            "listen": listen,
            "check_update": check_update,
            "download_update": download_update,
            "selftest": selftest,
            "install_cuda": install_runtime,
            "change_location": change_location,
            "test_cuda": gpu_probe,
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
        if reader is not None:
            emit(
                job, "progress", stage="Requête reçue par le moteur IA", phase="startup"
            )
        managed_install = method == "install_cuda" or (
            method == "install" and not params.get("cache")
        )
        with storage_lock() if managed_install else nullcontext():
            result = engines[method](
                params,
                lambda stage, **fields: emit(job, "progress", stage=stage, **fields),
            )
        emit(job, "result", result=result)
        return 0
    except Exception as exc:
        message = str(exc)
        if isinstance(exc, MemoryError) or "bad_alloc" in message.lower():
            message = "Mémoire insuffisante pour ce modèle. Augmentez le plafond de RAM ou choisissez un modèle plus petit."
        elif "out of memory" in message.lower() and "cuda" in message.lower():
            message = "Mémoire vidéo insuffisante. Libérez de la VRAM ou choisissez un modèle plus petit."
        emit(job, "error", code=type(exc).__name__, message=message)
        return 1
    finally:
        # Le parent ferme stdin après la réponse. Laisser le lecteur sortir avant
        # la fermeture du runtime natif, même si la requête s'est terminée en erreur.
        if reader is not None:
            reader.join(timeout=1)


if __name__ == "__main__":
    os._exit(main())
