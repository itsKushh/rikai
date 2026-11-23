# Validation

## Résultats de cette session

| Vérification | Résultat |
|---|---|
| Tests Rust sur Linux | Alignement d'un paquet traversant une pause et un silence ; récupération après piste manquante et exclusion du micro du mix ; édition, réindexation FTS et réouverture d'une version retraitée ; appel aller-retour Rust → processus Python et remontée d’une erreur structurée. |
| Rust sur cible Windows x64 | Contrôle de types de tout le workspace, y compris les cibles de test et l'UI GPUI-CE. Compilation optimisée et édition de liens de l'exécutable de diagnostic `meetingnotes-audio.exe` (PE32+, x86-64, cible gnullvm). |
| Python 3.12 | Tests du changement de voix dans une phrase, du chevauchement, de l'origine micro, du refus d'un profil ambigu, de l'absence de téléchargement ou d'enrôlement implicites, du canal de commande, de l'annulation, des preuves inconnues de la corrélation de reprises micro et du silence avec le vrai moteur VAD. |
| Dépendances natives | Import de Faster-Whisper 1.2.1, CTranslate2 4.9.1, Sherpa-ONNX 1.12.39 et llama-cpp-python 0.3.37 sur Linux. Compatibilité PyAV corrigée avec 15.1.0. Paquet `sherpa-onnx-core` explicitement verrouillé. |
| Extrait JFK de whisper.cpp | Téléchargement du modèle Rapide avec progression jusqu’à 78 203 619 octets, vérification des fichiers et transcription CPU des 11 secondes de l’extrait public par le worker. Le segment reconnu contient le mot « country ». |
| Worker autonome | Création d'un dossier PyInstaller Linux, lancement du binaire seul, échange IPC, détection de capacités, chargement des quatre bibliothèques natives et VAD. Les DLL Windows restent à qualifier. |
| Interface et installeur | La compilation complète depuis Linux s'arrête dans le compilateur de shaders GPUI-CE qui exige Windows pour DXBC. Son option de contrôle de types a été utilisée exclusivement pour `cargo check`. La fenêtre, le rendu GPU, l'installeur et le couple enceinte/micro requièrent la recette ci-dessous. |

Dans l’environnement cloud Linux, `bash scripts/setup-cloud.sh` installe Rust 1.100.0 sous `/workspace/.meetingnotes-tools`, synchronise le worker puis lance les suites Rust et Python. Pour une installation manuelle avec Rust et uv déjà présents :

```sh
uv sync --directory ai-worker --locked --python 3.12
cargo test --locked --workspace
uv run --directory ai-worker --locked python -m unittest discover -s tests -v
```

## Isolation par processus sur Windows — essai du 23 novembre 2026

L’utilisateur a exécuté le kit de diagnostic sur son poste Windows et transmis les sept mesures de `windows_isolation.py`. Les trois lecteurs émettent volontairement pendant toute la mesure : le choix des sources agit sur les pistes enregistrées, tandis que la lecture ordinaire reste audible.

| Processus sélectionnés | Observation dans le mix applicatif | Observation sur les pistes individuelles |
|---|---|---|
| 440 Hz seul | 440 Hz à 1495,38 ; les deux autres fréquences sous 0,010 | Même résultat sur la piste 1 |
| 997 Hz seul | 997 Hz à 1483,54 ; les deux autres fréquences sous 0,006 | Même résultat sur la piste 1 |
| 440 et 997 Hz | Niveaux 747,59 et 741,67 ; 1531 Hz à 0,0020 | 440 Hz sur la piste 1 et 997 Hz sur la piste 2 ; les fréquences exclues restent sous 0,010 sur chaque piste |

Les sept mesures dépassent le seuil de rejet de 40 dB. Le plus petit rapport mesuré entre un ton attendu et un ton exclu correspond à environ 104,2 dB, aux fréquences et dans la fenêtre analysées. Ces valeurs caractérisent cet essai sur ce poste. La configuration matérielle et le numéro de build n’ont pas été communiqués. Le scénario suivant concerne Discord ou TeamSpeak, le microphone physique et l’application installée.

## Recette Windows

Consigner le build Windows (20348 minimum), CPU/RAM, version du pilote audio, modèles exacts, périphériques, résultats et incidents. Utiliser des enregistrements de test autorisés et un casque. Archiver les manifestes, journaux et mesures avec l'accord des participants.

| Scénario | Critère |
|---|---|
| Isolation automatisée | Exécuter `py -3 tests/windows_isolation.py target/release/meetingnotes-audio.exe`. Trois processus jouent 440, 997 et 1531 Hz. D'abord sélectionner chacun des deux premiers isolément, puis les deux ensemble. Pour chaque piste et mix, les tons choisis dépassent un seuil de présence et les tons exclus sont atténués d'au moins 40 dB. |
| Discord + micro | Faire parler un correspondant, jouer de la musique dans Spotify et une vidéo dans un navigateur, déclencher une notification. Choisir uniquement Discord et le micro USB. Écouter séparément chaque WAV et le mix applicatif. Vérifier l'absence de musique, vidéo, notification et micro dans le mix. La voix locale doit être présente sur la seule piste micro, sous réserve de fuite acoustique physique. |
| Plusieurs applications | Choisir Discord et TeamSpeak, provoquer des repères temporels audibles, puis parler sur le micro. Vérifier le déroulement normal dans le casque, les temps de début des repères et l'ordre chronologique après transcription. Répéter après pause et reprise. |
| Fermeture et incidents | Débrancher puis rebrancher le micro. Une reconnexion est tentée uniquement sur le même endpoint. Fermer puis relancer le PID racine sélectionné : erreur explicite et nouvelle sélection manuelle. Vérifier que rien d'autre n'est capturé. Tuer le processus pendant la capture et utiliser « Récupérer les pistes lisibles ». Les originaux doivent rester intacts. |
| Endurance | Enregistrer deux heures. Contrôler le décalage micro/app au début et à la fin, les discontinuités et niveaux, l'espace disque, le temps de finalisation et la mémoire de diarisation. Un WAV mono 16 kHz/16 bits consomme environ 115 Mo/h. |
| Trois voix françaises | Utiliser un fichier annoté avec silence, noms, chiffres, changements brefs et chevauchements. Mesurer erreurs de mots, erreurs de locuteurs, frontières temporelles, attribution du micro et corrections. Réessayer avec un modèle manquant, annuler pendant une inférence et relancer. |
| Profils et inconnus | Créer deux profils avec consentement. Utiliser une autre réunion avec les deux voix et une voix inconnue. Vérifier les propositions, les ambiguïtés, le refus et la suppression des extraits/empreintes. Ne valider les seuils qu'après examen des faux rapprochements. |
| Compte rendu | Corpus annoté de deux décisions, trois actions, une échéance et une question ouverte. Vérifier chaque fait, responsable et date contre les segments cités. Étendre le corpus au-delà du contexte du modèle ; vérifier les décisions du premier bloc. Régénérer en conservant la version précédente. |
| Bibliothèque et installation | Sur un Windows propre, construire/installer, essayer un chemin contenant des espaces, tester les modèles CPU et les DLL CUDA séparément, les dialogues natifs, le clavier, le thème clair, la petite fenêtre et les exports. Corriger un passage, relancer, rechercher, filtrer, supprimer uniquement l'audio, mettre à jour puis désinstaller. Contrôler la conservation de la bibliothèque et la réaction de l'antivirus. |

### Utilitaire de capture manuelle

`meetingnotes-audio sources` imprime les sessions et entrées réellement détectées en JSON UTF-8. Pour un essai reproductible, conserver dans un fichier JSON UTF-8 **sans BOM** le tableau des seuls éléments voulus. Garder les champs de la source (notamment le PID et l'heure de création). Exécuter `meetingnotes-audio enregistrer "nouveau dossier" selection.json` et attendre REC. Saisir `pause`, `reprendre` ou `stop` sur une ligne. Une erreur conserve les pistes déjà écrites. Les pouvoirs administrateur et l'accès aux comptes Discord ne sont pas nécessaires.

