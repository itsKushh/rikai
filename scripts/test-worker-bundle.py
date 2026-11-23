import json
import os
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "ai-worker/tests"))
from test_pipeline import worker_exchange

command = [str(Path(sys.argv[1]).resolve())]


def invoke(method, params, expected):
    stages = []
    def progress(event):
        stages.append(event["stage"])
        print(f"{method} · {event['stage']}", flush=True)

    answer, code = worker_exchange(
        {"version": 1, "id": 1, "method": method, "params": params},
        command=command,
        on_progress=progress,
        timeout=600 if method in ("install_cuda","install") else 120,
    )
    if answer["event"] != expected or code != (0 if expected == "result" else 1):
        raise RuntimeError(f"Échec du worker embarqué : {answer}")
    return stages, answer


for method in ("capabilities", "selftest"):
    _, response = invoke(method, {}, "result")
    print(method, response["result"])

# Rejouer la demande envoyée après l'arrêt, en gardant le canal de contrôle
# ouvert. Un dossier modèle vide provoque une erreur locale déterministe.
with tempfile.TemporaryDirectory() as folder:
    root = Path(folder)
    (root / "sources.json").write_text(json.dumps({"tracks": []}), encoding="utf-8")
    stages, response = invoke(
        "transcribe", {"directory": folder, "settings": {"whisper": folder}}, "error"
    )
    assert stages[0] == "Requête reçue par le moteur IA", stages
    assert "Chargement du modèle de transcription" in stages, (stages, response)
    print("Transcription : initialisation, chargement et retour d’erreur vérifiés.")

# Même entrée IPC que le bouton d’installation. Le runner Windows sans GPU peut
# valider les DLL effectivement chargées et leurs versions par les API NVIDIA.
if sys.platform == "win32":
    with tempfile.TemporaryDirectory() as local:
        os.environ["LOCALAPPDATA"] = local
        invoke("install_cuda", {"consent": True}, "result")
        for method in ("capabilities", "install_cuda"):
            _, response = invoke(method, {"consent": True}, "result")
            if method == "capabilities":
                versions = response["result"]["runtime"]
                assert versions["cublas_major"] == 12, versions

    with tempfile.TemporaryDirectory() as cache:
        _, result=invoke("install",{"preset":"Traduction locale","consent":True,"cache":cache},"result")
        model=result["result"]["path"]
        _, result=invoke("translate",{"text":"Bonjour, merci beaucoup pour votre aide.","folder":model,"source":"","target":"en"},"result")
        assert result["result"]["source"]=="fr" and "thank" in result["result"]["text"].lower(),result
        print("M2M100 Windows : traduction française vers l’anglais vérifiée.")
