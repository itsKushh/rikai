import json
import subprocess
import sys

for method, expected in [("capabilities", "result"), ("selftest", "result")]:
    request = json.dumps({"version": 1, "id": 1, "method": method, "params": {}}) + "\n"
    p = subprocess.run(
        [sys.argv[1]], input=request, text=True, capture_output=True, timeout=120
    )
    answer = json.loads(p.stdout.splitlines()[-1])
    if p.returncode or answer["event"] != expected:
        raise RuntimeError(f"Échec du worker embarqué : {answer}")
    print(method, answer["result"])
