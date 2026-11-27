# Nouveautés (français)

Traduction de CHANGELOG.md pour la fenêtre « Nouveautés ». Même format, sections
traduites. Les versions absentes ici s'affichent en anglais.

## 0.4.8 - 2026-11-27

### Corrections
- Téléphones et tablettes : les tâches se déplacent maintenant à la main dans votre ordre. En « Mon ordre », chaque tâche a une poignée ≡ : faites-la glisser vers le haut ou le bas (glisser ailleurs sur la tâche fait toujours défiler la liste).

## 0.4.7 - 2026-11-27

### Nouveautés
- Rangez vos tâches dans votre propre ordre : faites glisser une tâche vers le haut ou le bas (ou clic droit ou appui long › Monter, Descendre, Placer en haut). Le tri passe au nouveau « Mon ordre », synchronisé avec vos autres appareils et les listes partagées.
- Sur téléphones et tablettes, les notes adhésives ont des boutons + / 🎨 / 🗑 plus grands, faciles à toucher.
- RustDo pour Linux sur ARM64 (Raspberry Pi de bureau, portables ARM) : .deb, .rpm, AppImage et .tar.gz.
- RustDo Hub pour Linux sans Docker : téléchargements prêts à l'emploi pour PC (x86_64) et Raspberry Pi (arm64), et un script d'installation qui configure le hub comme service lancé au démarrage de l'ordinateur.
- RustDo pour Linux existe aussi en .rpm (Fedora, openSUSE) et en .tar.gz pour toute distribution, avec un script d'installation qui ajoute RustDo au menu des applications.
- Installation pas à pas de RustDo Hub sur un NAS Synology (Container Manager), dans le guide du hub et dans « Comment fonctionnent la synchronisation et le partage » de RustDo.

### Corrections
- App web sur téléphones et tablettes Android : les lettres pouvaient paraître irrégulières, certaines coupées ou décalées (la puce graphique dessinait le texte avec trop peu de précision). Le texte est maintenant dessiné en pleine précision et reste aussi net que dans l'app Android.
- Android : dans Paramètres › Fond, le bouton « + Image… » manquait, donc impossible d'utiliser vos propres photos. Il est là maintenant et ouvre le sélecteur de photos de RustDo.
- Le texte est plus lisible sur les images de fond : les dates, les indications et les autres textes discrets ne se fondent plus dans l'image, en thème sombre comme en thème clair.

## 0.4.6 - 2026-11-27

### Nouveautés
- Images de fond : choisissez-en une pour toute l'app dans Paramètres › Fond, et donnez à chaque liste la sienne (clic droit ou appui long sur la liste › Fond…). Choisissez parmi les fonds intégrés ou votre propre image (sur Android, RustDo affiche vos photos une fois que vous l'autorisez). « Assombrir le fond » garde les tâches faciles à lire.
- Les fonds se synchronisent avec Synchroniser mes appareils, vos propres images aussi : une image choisie sur le PC apparaît aussi sur le téléphone. (Votre RustDo Hub doit être en version 0.4.6 ou ultérieure pour transporter les images.)
- Les notes adhésives peuvent aussi avoir une image : ouvrez le menu 🎨 d'une note › Image et choisissez-en une intégrée ou la vôtre. La couleur de la note la teinte pour que le texte reste lisible.
- Obtenir RustDo (dans le menu ⚙) présente toutes les façons d'avoir RustDo : Microsoft Store, l'installateur Windows, Android, Linux, l'app web et le Hub.
- Un bouton « Rejoignez la communauté RustDo sur Discord » en bas des listes, pour les idées, les bugs et l'aide.
- RustDo Hub peut fonctionner comme app Home Assistant : dans la boutique d'apps de Home Assistant, ajoutez le dépôt RustDo et installez RustDo Hub. Plus de détails dans le guide du hub.

### Corrections
- App web sur téléphones et tablettes : sur certains écrans, les lettres paraissaient tassées. « Texte net » (activé par défaut, dans le menu ⚙) fonctionne maintenant là aussi, comme dans l'app Android.
- L'image Docker de RustDo Hub fonctionne désormais quand son dossier de données appartient à root (fréquent sur les NAS et dans Home Assistant).

## 0.4.5 - 2026-11-26

### Corrections
- Android : après avoir masqué le clavier à l'écran (avec sa touche de masquage, le geste Retour ou en changeant d'application), toucher un champ de texte ne le faisait pas réapparaître. Désormais, toucher un champ ouvre toujours le clavier, et il revient quand vous retournez dans RustDo.
- « Ajouter une tâche » affiche un bouton Ajouter dès que vous tapez, pour enregistrer une tâche sans la touche Entrée du clavier.

## 0.4.4 - 2026-11-26

### Nouveautés
- Afficher la liste : une grande page facile à lire avec les tâches d'une liste et leurs notes, par exemple une liste de médicaments à montrer à votre médecin. Copiez-la en texte pour la coller dans un e-mail ou un message, ou sur ordinateur imprimez-la ou enregistrez-la en PDF. Le bouton se trouve à côté du nom de la liste, ou par clic droit (appui long) sur une liste.

### Améliorations
- Téléphones : un bouton Retour bien visible (une flèche dans un bouton arrondi) remplace le petit « < » dans le menu, les paramètres, les détails des tâches et les pense-bêtes.
- Android : le bouton et le geste Retour fonctionnent désormais dans RustDo. Retour ferme ce qui est ouvert, revient à la liste puis à Ma journée, et depuis Ma journée quitte l'application.
- Réorganisez vos listes : faites glisser une liste vers le haut ou le bas dans le menu, ou sur téléphone appuyez longuement dessus et choisissez Monter ou Descendre. Le nouvel ordre se synchronise avec vos autres appareils.
- Les nouvelles installations en allemand, espagnol ou français démarrent avec le format 24 heures, et le nom de la liste par défaut suit le changement de langue.
- Les « Nouveautés » s'affichent maintenant dans votre langue (allemand, espagnol ou français).
- RustDo Hub garde chaque jour une copie de ses données (les 14 derniers jours) dans un dossier backups à côté de hub.json.

## 0.4.3 - 2026-11-26

### Nouveautés
- RustDo parle désormais aussi espagnol et français : Paramètres › Langue propose Español et Français à côté d'English et Deutsch, et Automatique les choisit quand votre appareil les utilise. Les dates s'affichent aussi à la manière locale, par exemple « Miércoles, 25 de noviembre » et « Mercredi 25 novembre ».

## 0.4.2 - 2026-11-25

### Nouveautés
- Importez directement depuis Microsoft To Do : choisissez Importer depuis To Do dans le menu, puis Se connecter avec Microsoft. RustDo lit toutes vos listes, tâches, étapes, notes, dates d'échéance et rappels et vous montre ce qu'il va importer. Il lit seulement vos données To Do et ne les modifie jamais. Fonctionne sous Windows, Linux, Android et dans l'application web, avec les comptes Microsoft personnels et professionnels ou scolaires.
- Android peut maintenant importer depuis Microsoft To Do (en se connectant).

### Corrections
- Allemand : quelques messages longs encore en anglais sont maintenant traduits.

## 0.4.1 - 2026-11-25

### Nouveautés
- RustDo parle allemand : Paramètres › Langue propose Automatique, English et Deutsch. Automatique suit la langue de votre appareil.
- La prise en charge des langues est désormais intégrée, d'autres langues peuvent suivre.

## 0.4.0 - 2026-11-25

### Nouveautés
- Synchroniser mes appareils : gardez vos listes, tâches, pense-bêtes et réglages (thème, format de l'heure, tri) identiques sur tous vos appareils, y compris l'application web, grâce à votre propre RustDo Hub. Activez-le dans les Paramètres, sous Synchroniser mes appareils : votre premier appareil obtient une clé de synchronisation, et vos autres appareils utilisent la même adresse de hub et la même clé. Chaque appareil garde ses propres fenêtres de rappel et la position de ses pense-bêtes.
- Les listes partagées avec la famille restent séparées de votre propre synchronisation : les autres ne voient que la liste partagée, et vos autres appareils la rejoignent d'eux-mêmes.
- RustDo Hub 0.4.0 est nécessaire pour Synchroniser mes appareils (mettez à jour l'image Docker ou le hub Windows). Il permet aussi à l'application web de se connecter quand le hub a une adresse https://.
- Le guide du hub dans les Paramètres explique maintenant comment synchroniser vos propres appareils en plus du partage de listes.
