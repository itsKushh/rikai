# Rikai — architecture et audit préalable

## Sources examinées

| Source et révision | Observation dans le code | Décision |
|---|---|---|
| [Meet2Notes](https://github.com/estebanstifli/Meet2Notes/tree/5a9fb849338888fbfe7c3a897dc2d7f375f92a3d), MIT, Python ≥3.11 | `adapters/audio_capture/windows_wasapi.py`, `combined.py`, `mixer.py` et `docs/audio-capture.md` utilisent un endpoint de sortie, PyAudioWPatch et un mix micro/système. | Remplacer toute cette chaîne. Séparer l'origine locale des pistes distantes. |
| Transcription, diarisation et profils | `faster_whisper.py` charge CTranslate2 et conserve les mots. Sherpa associe segmentation et embeddings. Les adaptateurs Community-1 et Nemotron isolent les dépendances plus lourdes. `voice_sampling.py`, `profile_matching.py` et `docs/voice-sampling-validation.md` distinguent échantillon exploitable et identité probable. | Modèles locaux sélectionnés explicitement. Conserver les tours acoustiques bruts et attribuer le micro par provenance. Les noms confirmés sont des annotations. Évaluer les seuils sur des voix consentantes avant activation. |
| Pipeline, synthèse et stockage | `final_pipeline.py` orchestre les étapes, `speaker_text.py` rapproche les intervalles, `summary/llama_cpp.py` réduit les preuves par blocs. `infrastructure/database` utilise des migrations, des versions et une recherche sur le texte actif. Voir aussi `docs/architecture.md`. | Confier les transactions à Rust. Exécuter l'inférence hors UI. Conserver la provenance des données dérivées et les éditions. |
| [Frame](https://github.com/66HEX/frame/tree/a8e6fbe39ffbd3a3e6b6492e6ed96361298bcfd9), frame-app 0.35.0, GPL-3.0-or-later | Rust 2024, outil 1.95.0. `frame-app/src/app/chrome` sépare navigation, zones de déplacement et contrôles de fenêtre. Composants avec survol, focus, indicateur d'onglet animé. Palette et métriques dans `src/theme`, Inter sous OFL 1.1. GPUI-CE modifié et embarqué par Frame. `inno/frame.iss` et xtask organisent le packaging. | Utiliser GPUI-CE amont sous Apache-2.0. Créer nos propres composants, conserver les contrôles Windows. Aucune reprise des sources, icônes, thèmes, fontes ou correctifs de Frame. Inter peut être redistribuée avec son avis OFL depuis sa propre distribution. |
| [Microsoft ApplicationLoopback](https://github.com/microsoft/Windows-classic-samples/tree/434f6002bdf9cf9829406c3ff2b33387982d6168/Samples/ApplicationLoopback), MIT | Activation asynchrone avec `AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK` sur le périphérique virtuel, puis lecture de paquets via IAudioCaptureClient. Windows build 20348 minimum. | Utiliser le binding Rust wasapi 0.27.0, dont la fabrique appelle cette API via windows 0.62.2. L'unique mode applicatif est INCLUDE_TARGET_PROCESS_TREE. L'entrée micro doit être un endpoint de capture. |

Étude documentaire et inspection des sources sur Linux. La preuve d'isolation acoustique nécessite une session Windows réelle avec des émetteurs concurrents. La licence d'un moteur ne couvre pas les poids de ses modèles. Avant redistribution, vérifier les avis de chaque binaire, modèle et DLL, conserver les licences et conditions d'accès. Community-1 exige l'acceptation des conditions du modèle. Nemotron 3 est une option à qualifier séparément (jusqu'à huit voix).

## Frontières

Le domaine manipule une sélection explicite et des temps relatifs à un enregistrement. Le module audio énumère les sessions sur chaque sortie, agrège les processus apparentés d'un même exécutable et ouvre une session WASAPI par racine choisie. Une racine inclut tous ses enfants. Un onglet Chrome et un utilisateur Discord ne constituent pas des sources isolables. Un nouveau PID racine exige une nouvelle sélection. Ne jamais choisir un processus sur son seul nom après redémarrage.

Les threads audio COM MTA normalisent en PCM 16 kHz mono via le convertisseur partagé Windows, consignent les incidents et écrivent progressivement par piste. Les horodatages QPC des paquets permettent d'aligner les pistes. La bibliothèque et l'interface ne dépendent pas des objets COM. Les fichiers de capture résident sous le profil utilisateur. Une erreur de capture doit rester visible et les fragments déjà écrits doivent rester récupérables.

## Contrat du worker

Processus enfant lancé directement, arguments séparés, stdin/stdout UTF-8, une enveloppe JSON par ligne. Aucun port ni service à authentifier. Le journal du processus utilise stderr. Un travail possède un identifiant, une méthode, des paramètres locaux et des événements de progression suivis d'un seul résultat ou d'une erreur. Le propriétaire peut annuler et arrêter le processus. L'écriture SQLite reste dans Rust. Les résultats d'une nouvelle exécution doivent être validés puis publiés en une transaction. Les chemins et les poids sont locaux, sans téléchargement implicite. Les fonctionnalités prévues et celles vérifiées sont distinguées dans le plan.



### Dépendances de la version Windows

Les pictogrammes utilisés par les contrôles sont sélectionnés dans Nucleo UI 1.9.0 fourni par le propriétaire du projet. Les deux images Rikai fournissent le logo de la barre de titre et les tailles de l’icône Windows. GPUI charge le groupe d’icônes 1 dans l’exécutable. Inno Setup et le raccourci utilisent le même fichier ICO.

Le binaire Windows de CTranslate2 4.9.1 est construit avec CUDA 12.8, le chargement dynamique de cuBLAS et les convolutions CUDA internes. L’installation NVIDIA facultative vérifie les roues Windows de cuBLAS 12.8.5.5 et de sa dépendance NVRTC 12.8.93, puis extrait leurs DLL et licences dans le profil utilisateur. Le worker charge les DLL par chemin absolu avant le calcul. Le pilote reste fourni par NVIDIA. [Configuration de compilation CTranslate2](https://github.com/OpenNMT/CTranslate2/blob/v4.9.1/python/tools/prepare_build_environment_windows.sh) · [Distribution NVIDIA](https://pypi.org/project/nvidia-cublas-cu12/12.8.5.5/)
