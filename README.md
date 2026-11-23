# MeetingNotes

Application native Windows x64 pour enregistrer manuellement une réunion, avec une piste par application choisie et une piste de microphone indépendante. Les sources de la première version sont disponibles. Le test Windows d’isolation a réussi avec trois processus émetteurs et une ou deux applications sélectionnées. La recette Discord + microphone, le test de l’application installée et la qualification des modèles restent ouverts.

## Essayer la capture sur Windows

Le kit de diagnostic contient `meetingnotes-audio.exe`, compilé pour Windows x64, et `windows_isolation.py`. Sur un poste Windows 11 avec une sortie audio active et Python, ouvrir PowerShell dans le dossier extrait et exécuter :

```powershell
py -3 windows_isolation.py .\meetingnotes-audio.exe
```

Ce test joue trois sons et mesure leurs niveaux dans les fichiers capturés. Pour tester Discord, le micro ou un casque USB, suivre [la recette Windows](docs/VALIDATION.md). Le sélecteur cible des arborescences de processus. Les utilisateurs d'un appel Discord partagent sa piste applicative. Pour un navigateur, tous les onglets du processus choisi peuvent être audibles.

## Construire l'application et l'installeur

Sur Windows x64, installer Rust via rustup, Visual Studio Build Tools avec **Développement Desktop en C++** et le SDK Windows, uv, CMake et Inno Setup 6. Ouvrir une Developer PowerShell x64 dans ce dépôt.

```powershell
.\scripts\build-windows.ps1
```

Le script exécute les tests, compile les shaders et les deux exécutables, assemble le worker Python 3.12 avec ses DLL en mode PyInstaller onedir, vérifie le worker assemblé et appelle Inno Setup. La sortie attendue est `dist\MeetingNotes-Setup-x64.exe`. Le workflow GitHub Actions **Compilation Windows x64** exécute le même script à chaque envoi sur `main`, sur demande ou sur une pull request. L'installeur et les DLL embarquées doivent encore être essayés sur un poste propre. Les poids des modèles sont installés séparément.

Pour développer, depuis la racine :

```powershell
uv sync --directory ai-worker --locked --python 3.12 --extra summary
cargo run --locked -p meetingnotes
```

## Parcours

La barre supérieure donne accès à l’espace de travail, à la bibliothèque et aux profils vocaux. L’inspecteur regroupe la session, les sources audio et les modèles. **Paramètres** ouvre le tiroir de configuration. Les thèmes sombre et clair, les panneaux et les contrôles GPUI reprennent le style Graphite de [Frame](https://github.com/66HEX/frame/blob/a8e6fbe39ffbd3a3e6b6492e6ed96361298bcfd9/frame-app/assets/themes/theme.css).

1. Dans **Paramètres**, choisir un dossier Faster-Whisper local ou sélectionner **Rapide**, **Équilibré** ou **Haute qualité**. Pour télécharger le modèle sélectionné, cliquer sur **Télécharger** dans l’écran de confirmation de MeetingNotes. La barre indique le volume reçu sur le total attendu. Les fichiers déjà vérifiés sont comptés et réutilisés. La diarisation utilise deux fichiers compatibles avec Sherpa (segmentation et embeddings), la synthèse locale un fichier GGUF de conversation. Le réglage **Calcul** permet de choisir CPU ou CUDA. En cas d’indisponibilité des bibliothèques NVIDIA, la transcription reprend sur CPU au bloc audio en cours. Ce réglage s’applique aussi aux réunions déjà enregistrées.
2. Dans **Nouvelle réunion**, saisir le titre, sélectionner les sessions applicatives et éventuellement un micro. Si une application manque, lui faire jouer du son. Cliquer sur **Tester les niveaux**, parler ou lancer du son et ajuster chaque source avec **−** et **+**. Viser des crêtes entre −18 et −6 dBFS. Le gain est repris lors de l’enregistrement. Informer les participants, puis démarrer. Les pistes s'écrivent en PCM mono 16 kHz sur disque.
3. À l'arrêt, les pistes sont finalisées et les applications mixées séparément du micro. Si un modèle de transcription est configuré, l’écran indique son chargement, la piste en cours, le temps traité et le résultat. La fiche permet aussi de relancer le traitement. Le micro prend le nom choisi dans les paramètres. Les tours acoustiques ambigus restent signalés.
4. Depuis la fiche, relire les passages, les écouter, corriger le texte, renommer un intervenant pour cette réunion ou créer un profil vocal avec autorisation. Les propositions de noms doivent être confirmées. Le résumé local cite les segments et possède son propre historique. L'export propose Markdown, texte ou JSON.
5. **Mes réunions** recherche titres, paroles et intervenants. Filtrer par projet, tag et date. Un résultat de parole ouvre le passage correspondant.

## Données et confidentialité

Les données sont stockées dans `%LOCALAPPDATA%\MeetingNotes` : bibliothèque SQLite, pistes, cache des poids téléchargés et journal technique. Les extraits biométriques autorisés et les embeddings restent dans SQLite. L'installation par utilisateur et la désinstallation de l'application laissent cette bibliothèque dans le profil Windows. Pour une sauvegarde complète, fermer l'application puis copier le dossier entier. La suppression du dossier après désinstallation efface les données locales.

Par défaut l'inférence lit les modèles locaux. Le fournisseur distant compatible `chat/completions` demande une URL HTTPS, un modèle, un identifiant générique Windows nommé `MeetingNotes/hôte[:port]` et une confirmation d'envoi pour chaque synthèse. Seuls les textes, locuteurs et identifiants nécessaires à la synthèse sont transmis. Le téléchargement explicite des modèles contacte le catalogue public. Les erreurs de réseau et d'inférence laissent la réunion disponible.

## Repères

- [Architecture, sources examinées et licences](docs/ARCHITECTURE.md)
- [Plan, fonctionnalités et limites de cette version](docs/IMPLEMENTATION_PLAN.md)
- [Contrat Rust–Python](docs/IPC.md)
- [Tests exécutés et recette Windows](docs/VALIDATION.md)

