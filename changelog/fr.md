# Nouveautés (français)

Traduction de CHANGELOG.md pour la fenêtre « Nouveautés ». Même format, sections
traduites. Les versions absentes ici s'affichent en anglais.

## 0.4.4 - 2026-11-28

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
