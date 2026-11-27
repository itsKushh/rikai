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
    saw_generation = False
    def progress(event):
        nonlocal saw_generation
        if event.get("generated_chars", 0) > 0:
            saw_generation = True
        if stages and stages[-1] == event["stage"]:
            return
        stages.append(event["stage"])
        print(f"{method} · {event['stage']}", flush=True)

    answer, code = worker_exchange(
        {"version": 1, "id": 1, "method": method, "params": params},
        command=command,
        on_progress=progress,
        timeout=600 if method in ("install_cuda","install") else 300 if method == "summarize" else 120,
    )
    if answer["event"] != expected or code != (0 if expected == "result" else 1):
        raise RuntimeError(f"Échec du worker embarqué : {answer}")
    if method == "summarize":
        assert saw_generation, "No streamed summary progress from the packaged worker"
    return stages, answer


# Exercise PyAV through the packaged IPC worker, including resampling and its WAV manifest.
with tempfile.TemporaryDirectory() as folder:
    import wave
    root = Path(folder)
    source = root / "exemple.wav"
    with wave.open(str(source), "wb") as out:
        out.setparams((1, 2, 8000, 0, "NONE", ""))
        out.writeframes(b"\x00\x00" * 8000)
    _, probe = invoke("probe_media", {"path": str(source), "thumbnail": str(root / "thumb.png")}, "result")
    assert probe["result"]["video"] is False, probe
    _, decoded = invoke("import_media", {"path": str(source), "directory": str(root)}, "result")
    assert decoded["result"]["duration_ms"] == 1000 and (root / "finalise").is_file(), decoded

for method in ("capabilities", "selftest"):
    _, response = invoke(method, {}, "result")
    print(method, response["result"])
    if sys.platform == "win32" and method == "selftest":
        cpu = response["result"]["llama_cpu"]
        assert "AVX2 = 1" in cpu and "AVX512" not in cpu and "AVX_VNNI" not in cpu, cpu

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

    # Exercise native quantized kernels and constrained generation, beyond import.
    with tempfile.TemporaryDirectory() as cache:
        from unittest.mock import patch
        with patch.dict(os.environ, HF_HUB_OFFLINE="1", TRANSFORMERS_OFFLINE="1"):
            _, result = invoke("install", {"preset": "Résumé contextualisé", "consent": True, "cache": cache}, "result")
        stages, result = invoke("summarize", {
            "settings": {"gguf": result["result"]["path"]},
            "segments": [
                {"id": i, "speaker": "Luc", "text": ("Je dois envoyer le devis vendredi. " * 300) if i == 1 else "Oui."}
                for i in range(1, 33)
            ] + [{"id": 33, "speaker": "Eva", "text": "Je dois réserver le studio lundi."}],
        }, "result")
        assert "Synthèse de l’ensemble de l’échange" in stages, stages
        assert not any(stage.startswith("Lecture des sujets") for stage in stages), stages
        assert result["result"]["body"].startswith("## En bref") and "## Sujets abordés" in result["result"]["body"], result
        assert not result["result"]["partial"], result
        assert result["result"]["body"] and {1, 33} <= set(result["result"]["evidence"]), result
