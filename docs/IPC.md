# IPC v1

Rust ouvre directement le worker installé dans `worker/rikai-worker.exe`, ou le Python de `ai-worker/.venv` en développement. Un processus exécute un travail. stdin et stdout sont des tubes privés UTF-8, délimités par des sauts de ligne. Le worker duplique stdout pour le protocole et dirige les écritures natives vers stderr. Rust vérifie version, identifiant, type d'événement et code de sortie. Aucun port réseau n'est ouvert.

Exemple sans modèle :

```json
{"version":1,"id":17,"method":"ping","params":{}}
{"version":1,"id":17,"event":"result","result":{"protocol":1}}
```

Pour `transcribe`, les paramètres sont `directory` (dossier contenant le manifeste et les pistes), `settings` (dossier Whisper, langue, CPU/CUDA, options de voix et modèles) et, uniquement sur activation, `profiles`. Le résultat contient `segments`, `turns`, `proposals` et `device`, le périphérique utilisé pour terminer la transcription. En cas d’erreur du moteur CUDA, le worker peut terminer sur CPU et signale la reprise par un événement de progression. Un segment est défini dans `meeting-storage::Segment`, avec provenance, temps en millisecondes et mots horodatés ; SQLite lui attribue un identifiant lors de la transaction. La synthèse reçoit la transcription active avec ses identifiants persistés et renvoie `body` et `evidence`. L'enrôlement exige `consent:true`. L'installation du modèle exige également un consentement et résout une révision immuable.

Durant un travail, Rust garde stdin ouvert. Après la réponse terminale, Rust ferme ce tube pour libérer le lecteur de commandes, puis attend au maximum dix secondes que le processus se termine. Sous Windows, le lecteur interroge le tube avant de lire les octets disponibles pour que les DLL natives puissent s’initialiser. Une notification `event:"progress"` contient une étape réelle. Pendant un téléchargement, `received` et `total` indiquent les octets du modèle, fichiers locaux vérifiés inclus. Le superviseur borne le démarrage à 90 secondes et le chargement du modèle de transcription à cinq minutes depuis la dernière réponse. Une erreur terminale porte `event:"error"`, `code` et `message`. Une commande `{"id":17,"method":"cancel"}` ou `shutdown` sur le même tube interrompt le processus ; la perte du parent ferme le tube. En cas de blocage dans une bibliothèque native, Rust peut tuer le worker après le délai de grâce. Un résultat partiel du worker n'est jamais publié comme transcription terminée. Le cache des modèles est distinct des processus d'inférence.



Le bouton de vérification envoie `test_cuda` avec les mêmes `settings` que la transcription. Le worker exécute un encodeur et un décodeur en CUDA et renvoie `device` et `compute_type`. `install_cuda` requiert `consent: true` et émet le volume transféré pour les bibliothèques NVIDIA. En transcription, `strict_cuda` exige le GPU, `cpu_threads` limite les threads du moteur, `ram_gib` borne la mémoire engagée du processus Windows et `gpu_duty` règle la part de temps de calcul. Le champ facultatif `vocabulary` complète les termes de la seconde écoute française.
