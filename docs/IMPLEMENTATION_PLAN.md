# Plan de livraison

1. **Qualification de la capture.** Énumérer les sessions, regrouper les PID, choisir les micros, enregistrer manuellement en PCM séparé. Contrôler le build Windows et la provenance avant l'activation. Vérifier les paquets silencieux, les pertes, le débranchement et la sortie partielle. Condition de livraison : passer le scénario Discord + microphone avec deux émetteurs exclus.
2. **Interface.** Relier des contrôles GPUI-CE aux sources réelles, à l'état d'enregistrement et à ses erreurs. Tester le clavier, le redimensionnement et les niveaux sur Windows.
3. **Transcription.** Worker séparé, modèle Faster-Whisper local, mots horodatés et fusion par temps. Annuler et rejouer sans perdre les corrections.
4. **Diarisation et profils.** Qualifier Sherpa sur trois voix et chevauchements. Séparer les références de profils consentis des labels temporaires. Valider les inconnus et la suppression. Qualifier Community-1 et Nemotron séparément.
5. **Synthèse.** Générer des faits appuyés par des segments, réduire par blocs, vérifier les actions et décisions. Valider les limites de contexte et la régénération.
6. **Bibliothèque.** SQLite/FTS5, provenance et versions, recherche, écoute et exports. Tester fermeture, réouverture et interruption.
7. **Distribution.** Build x64, worker onedir, modèles séparés, installeur par utilisateur, vérification sur Windows propre. Mesurer deux heures et tester la mise à jour avec données existantes.

Pour chaque étape, distinguer les tests reproductibles sans périphérique, le contrôle de compilation pour Windows et la recette sur matériel. Le succès d'un test de traitement de fichiers ne valide pas la capture du système.


## État de la première version

| Domaine | Implémenté dans le dépôt | Limites ouvertes |
|---|---|---|
| Capture | Sessions audio sur les sorties actives, regroupement par exécutable et ascendance, vérification du PID/temps de création/image, rejet des sélections imbriquées, micro par identifiant, pistes PCM, mix applicatif, niveaux, pause/reprise/arrêt, abandon confirmé, QPC et copie de récupération. | Recette matérielle indispensable. Une racine redémarrée demande une nouvelle sélection. Les applications sans session audio peuvent devoir jouer un son pour apparaître. La reconnexion micro a une fenêtre de 15 s. Pas d'isolation garantie par onglet. |
| Interface | Fenêtre Windows GPUI-CE, navigation native, Inter, palettes graphite et claire, sources, REC, progression réelle, opérations destructives confirmées et préférences. | Validation visuelle et accessibilité au clavier sur Windows à effectuer. Barre de titre standard Windows. |
| IA | Worker séparé, transcription par blocs de 180 s et horodatage par mot, Sherpa sur le mix, alignement des mots, profils locaux avec extraits consentis et empreinte du modèle, propositions à confirmer. | Les coupures entre blocs peuvent affecter un mot. Le moteur de diarisation charge la forme d'onde distante complète en mémoire. Les seuils cosinus 0,77 et marge 0,10 sont provisoires. Les reprises acoustiques possibles sont signalées seulement si le texte concorde, les temps sont proches et les formes d’onde fortement corrélées. Les deux interventions sont conservées. La sensibilité reste à mesurer. |
| Résumés | llama.cpp local ou API HTTPS confirmée par envoi, extraction par blocs, consolidation bornée, préservation des décisions/actions, validation des identifiants de preuve, édition et historique. | Vérifier la fidélité sur corpus annoté. Une référence valide ne démontre pas à elle seule que le fait cité est exact. Un modèle sans format de conversation compatible peut échouer. |
| Bibliothèque | SQLite avec migrations, index FTS5, filtres, tags, versions, éditions conservées, profils distincts des labels acoustiques, écoute de passages via Windows, export, suppression d'audio et récupération. | Les corrections sont recopiées si le segment brut, ses bornes et sa piste correspondent exactement. Sinon elles restent accessibles dans la version antérieure. Les exports utilisent la version active. Le dossier de données est pour cette version celui du profil Windows ; sauvegarde à froid documentée. |
| Distribution | Cargo.lock, uv.lock, script de build, workflow Windows, worker onedir, recette Inno Setup par utilisateur et icône originale. | Installeur à construire sur Windows. La taille et les DLL requises du bundle Windows, la signature et l'installation sur machine propre ne sont pas validées. Le cache des modèles est géré par Hugging Face ; l'UI ne propose pas encore de quota ni de nettoyage sélectif. |

### Moteurs optionnels

Le moteur de diarisation intégré est Sherpa-ONNX. Les références étudiées isolent Community-1 et Nemotron dans des environnements distincts. Leur ajout doit valider les distributions PyTorch/NeMo Windows, l'accès et les conditions de chaque modèle, la limite de huit voix de Nemotron et un corpus commun avant de les proposer dans les paramètres. Ils ne sont pas livrés dans ce premier paquet. Les poids de segmentation, embeddings et synthèse sont fournis par l'utilisateur ; le téléchargement intégré couvre les trois profils Faster-Whisper.

### Distribution et provenance

La police Inter vient directement de google/fonts, révision 06bfe0d62a98375cb09c942fa18e68bd32cff057, avec son texte OFL dans assets/inter. L'icône est une création originale. L'assemblage collecte les avis des paquets Cargo et Python. Pour une diffusion publique, examiner les notices et obligations de redistribution des DLL et des codecs réellement livrés ; notamment les binaires FFmpeg embarqués par PyAV, les bibliothèques ONNX et les dépendances transitives. La compilation Linux de PyAV annonce LGPL version 3 ou ultérieure pour ses bibliothèques FFmpeg. Vérifier la configuration du wheel Windows et conserver les sources ou offres requises pour ses propres binaires. Les poids ne font pas partie de l'installeur.
