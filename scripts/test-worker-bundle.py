import json
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "ai-worker/tests"))
from test_pipeline import worker_exchange

command = [str(Path(sys.argv[1]).resolve())]


def invoke(method, params, expected):
    stages = []
    answer, code = worker_exchange(
        {"version": 1, "id": 1, "method": method, "params": params},
        command=command,
        on_progress=lambda event: stages.append(event["stage"]),
        timeout=120,
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
