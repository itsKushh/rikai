# Rikai

Application native Windows x64 pour enregistrer manuellement une réunion, avec une piste par application choisie et une piste de microphone indépendante. Les sources de la première version sont disponibles. Le test Windows d’isolation a réussi avec trois processus émetteurs et une ou deux applications sélectionnées. La recette Discord + microphone, le test de l’application installée et la qualification des modèles restent ouverts.

## Installer Rikai

Télécharger `Rikai-Setup-x64.exe` depuis les [versions privées du projet](https://github.com/itsKushh/kpiche/releases/latest). Fermer la version précédente avant de lancer l’installeur. Au premier démarrage, Rikai reprend dans son nouveau dossier la bibliothèque, les pistes et les modèles installés.

**Paramètres → Interface** suit la langue de Windows par défaut. Les six langues de l’interface sont disponibles hors ligne. La langue de transcription se règle séparément dans **Calcul**.

**Paramètres → Modèles → Résumé local** télécharge Qwen2.5-3B-Instruct au format GGUF (2,10 Go). Un GGUF contient les poids et les métadonnées nécessaires pour générer du texte localement. Le bouton **Générer le résumé local** d’une réunion transcrite propose aussi l’installation, puis reprend la demande. Le choix d’un GGUF personnel reste disponible.

**Transcription → Traduire le passage** ouvre le choix de la langue cible. La recherche accepte le nom local, le nom d’origine et le code de langue. À la première utilisation, Rikai propose M2M100 (496 Mo). Les 100 langues cibles sont indépendantes de la langue d’interface. Le modèle utilise la langue de la transcription quand elle est connue et identifie sinon la langue du texte. Les traductions sont enregistrées à côté de l’original. Corriger le texte rend ses traductions précédentes caduques.

**Paramètres → Mises à jour → Connecter GitHub** ouvre GitHub et la boîte de saisie Windows. Créer un jeton finement limité au dépôt `itsKushh/kpiche`, avec **Contents → Read-only**, puis le coller dans le champ mot de passe de la fenêtre Rikai. Le Gestionnaire d’identifiants Windows le conserve sous `Rikai/GitHub`. Une fois connecté, Rikai peut vérifier les versions au démarrage, afficher les notes et télécharger l’installeur après confirmation. L’empreinte SHA-256 doit correspondre à celle de la version GitHub. Le choix final installe la version et redémarre Rikai ou le laisse fermé.

Pour publier une version, mettre à jour le numéro du workspace, ajouter `releases/vX.Y.Z.md` et envoyer sur `main`. Après les tests Windows, le workflow joint l’installeur à la version privée correspondante. Une version déjà publiée reste immuable.

## Essayer la capture sur Windows

Le kit de diagnostic contient `rikai-audio.exe`, compilé pour Windows x64, et `windows_isolation.py`. Sur un poste Windows 11 avec une sortie audio active et Python, ouvrir PowerShell dans le dossier extrait et exécuter :

```powershell
py -3 windows_isolation.py .\rikai-audio.exe
```

Ce test joue trois sons et mesure leurs niveaux dans les fichiers capturés. Pour tester Discord, le micro ou un casque USB, suivre [la recette Windows](docs/VALIDATION.md). Le sélecteur cible des arborescences de processus. Les utilisateurs d'un appel Discord partagent sa piste applicative. Pour un navigateur, tous les onglets du processus choisi peuvent être audibles.

## Construire l'application et l'installeur

Sur Windows x64, installer Rust via rustup, Visual Studio Build Tools avec **Développement Desktop en C++** et le SDK Windows, uv, CMake et Inno Setup 6. Ouvrir une Developer PowerShell x64 dans ce dépôt.

```powershell
.\scripts\build-windows.ps1
```

Le script exécute les tests, compile les shaders et les deux exécutables, assemble le worker Python 3.12 avec ses DLL en mode PyInstaller onedir, vérifie le worker assemblé et appelle Inno Setup. La sortie attendue est `dist\Rikai-Setup-x64.exe`. Le workflow GitHub Actions **Compilation Windows x64** exécute le même script à chaque envoi sur `main`, sur demande ou sur une pull request. L'installeur et les DLL embarquées doivent encore être essayés sur un poste propre. Les poids des modèles sont installés séparément.

Pour développer, depuis la racine :

```powershell
uv sync --directory ai-worker --locked --python 3.12 --extra summary
cargo run --locked -p rikai
```

## Parcours

La barre supérieure donne accès à l’espace de travail, à la bibliothèque et aux profils vocaux. L’inspecteur regroupe la session, les sources audio et les modèles. **Paramètres** ouvre le tiroir de configuration. Les thèmes sombre et clair, les panneaux et les contrôles GPUI reprennent le style Graphite de [Frame](https://github.com/66HEX/frame/blob/a8e6fbe39ffbd3a3e6b6492e6ed96361298bcfd9/frame-app/assets/themes/theme.css).

1. Dans **Paramètres**, choisir un dossier Faster-Whisper local ou sélectionner **Rapide**, **Équilibré**, **Haute qualité** ou **Précision Turbo**. Pour télécharger le modèle sélectionné, cliquer sur **Télécharger** dans l’écran de confirmation de Rikai. La barre indique le volume reçu sur le total attendu. Les fichiers déjà vérifiés sont comptés et réutilisés. La diarisation utilise deux fichiers compatibles avec Sherpa (segmentation et embeddings), la synthèse locale un fichier GGUF de conversation. Le profil **Rapide** utilise Faster-Whisper base. Si la version tiny est déjà installée, sélectionner à nouveau **Rapide** pour obtenir base. **Équilibré** utilise small pour les échanges plus difficiles. En mode **Français**, un passage contenant des caractères chinois, japonais ou coréens déclenche une seconde écoute ciblée. Le réglage **Calcul** permet de choisir CPU ou CUDA. Avec **CUDA**, **Exiger le calcul sur GPU** interrompt le travail en cas d’indisponibilité. Désactiver cette option autorise la reprise sur CPU au bloc en cours. Ce réglage s’applique aussi aux réunions déjà enregistrées.
2. Dans **Nouvelle réunion**, saisir le titre, sélectionner les sessions applicatives et éventuellement un micro. Si une application manque, lui faire jouer du son. Cliquer sur **Tester les niveaux**, parler ou lancer du son et ajuster chaque source avec **−** et **+**. Viser des crêtes entre −18 et −6 dBFS. Le gain est repris lors de l’enregistrement. Le microphone est ouvert dans le format natif de l’entrée Windows et converti en continu en PCM mono à 16 kHz. Informer les participants, puis démarrer. Les pistes s'écrivent en PCM mono 16 kHz sur disque.
3. À l'arrêt, les pistes sont finalisées et les applications mixées séparément du micro. Si un modèle de transcription est configuré, l’écran indique son chargement, la piste en cours, le temps traité et le résultat. La fiche permet aussi de relancer le traitement. Le micro prend le nom choisi dans les paramètres. Les tours acoustiques ambigus restent signalés.
4. Dans l’onglet **Transcription**, la barre de défilement parcourt tous les passages. Chaque carte permet de lire, mettre en pause, cliquer sur la ligne de temps, exporter le passage en WAV ou cliquer sur les mots pour les corriger. Depuis la fiche, renommer un intervenant pour cette réunion ou créer un profil vocal avec autorisation. Les propositions de noms doivent être confirmées. Le résumé local cite les segments et possède son propre historique. L'export propose Markdown, texte ou JSON.
5. **Mes réunions** recherche titres, paroles et intervenants. Filtrer par projet, tag et date. Un résultat de parole ouvre le passage correspondant.

## Transcrire une conversation française sur GPU

1. Dans **Paramètres → Calcul**, sélectionner **CUDA**. Choisir **Installer le moteur NVIDIA** puis confirmer. Le premier téléchargement représente environ 641 Mo. Le worker vérifie les empreintes SHA-256 des distributions Windows de NVIDIA et installe les DLL dans le profil de l’utilisateur. Windows doit disposer d’un pilote NVIDIA compatible CUDA 12.
2. Choisir **Précision Turbo** pour télécharger Whisper large-v3-turbo multilingue (environ 1,62 Go). Sélectionner **Français** puis **Vérifier CUDA avec ce modèle**. Le test exécute l’encodeur et le décodeur sur la carte avec une seconde de silence et affiche le type de calcul obtenu.
3. Le champ **Vocabulaire français** accepte des noms et expressions séparés par des virgules. Sur un passage incertain, une seconde écoute utilise ce vocabulaire avec des termes familiers. Les mots reconnus avec confiance conservent leur transcription.
4. Les curseurs règlent les threads de calcul de la transcription, le plafond de mémoire engagée par son processus Windows et la part de temps consacrée au calcul GPU. Le plafond de mémoire peut interrompre un modèle trop volumineux. La part GPU espace les séquences d’inférence. Les mesures de charge et de mémoire disponible sont actualisées toutes les trois secondes dans les paramètres. Les réglages s’appliquent au prochain traitement.

## Données et confidentialité

Les données sont stockées dans `%LOCALAPPDATA%\Rikai` : bibliothèque SQLite, pistes, cache des poids téléchargés et journal technique. Les extraits biométriques autorisés et les embeddings restent dans SQLite. L'installation par utilisateur et la désinstallation de l'application laissent cette bibliothèque dans le profil Windows. Pour une sauvegarde complète, fermer l'application puis copier le dossier entier. La suppression du dossier après désinstallation efface les données locales.

Par défaut l'inférence lit les modèles locaux. Le fournisseur distant compatible `chat/completions` demande une URL HTTPS, un modèle, un identifiant générique Windows nommé `Rikai/hôte[:port]` et une confirmation d'envoi pour chaque synthèse. Seuls les textes, locuteurs et identifiants nécessaires à la synthèse sont transmis. Le téléchargement explicite des modèles contacte le catalogue public. Les erreurs de réseau et d'inférence laissent la réunion disponible.

## Repères

- Le code original de Rikai est propriétaire. Sa réutilisation nécessite une autorisation écrite. [Conditions de consultation et de réutilisation](LICENSE)
- [Architecture, sources examinées et licences](docs/ARCHITECTURE.md)
- [Plan, fonctionnalités et limites de cette version](docs/IMPLEMENTATION_PLAN.md)
- [Contrat Rust–Python](docs/IPC.md)
- [Tests exécutés et recette Windows](docs/VALIDATION.md)

