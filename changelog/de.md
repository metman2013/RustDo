# Neuigkeiten (Deutsch)

Übersetzung von CHANGELOG.md für das Fenster „Neuigkeiten“. Gleiches Format, Abschnitte
übersetzt. Versionen, die hier fehlen, erscheinen auf Englisch.

## 0.4.11 - 2026-11-28

### Behoben
- Tablets und Smartphones: Die Bildschirmtastatur konnte das Feld verdecken, in das du tippst (zum Beispiel beim Einschalten der Synchronisierung). Dialoge rücken jetzt in den Platz über der Tastatur und lassen sich dort scrollen, und die Web-App verkleinert sich über der Tastatur, statt dahinter zu verschwinden.
- Tablets im Querformat (Android-App und Web-App): Das Menü ließ sich nicht bis zu „Neue Liste“, Import, Haftnotizen und dem unteren Ende scrollen. Auf niedrigen Bildschirmen scrollt jetzt das ganze Menü.

## 0.4.10 - 2026-11-28

### Behoben
- Android: Kopieren erreichte die Zwischenablage des Telefons nicht, sodass ein kopierter Sync-Schlüssel, Freigabecode, eine Liste oder Sicherung nicht in anderen Apps eingefügt werden konnte. Jede Kopieren-Schaltfläche nutzt jetzt die Zwischenablage von Android und zeigt kurz „✔ Kopiert“.
- Die Felder für Hub-Adresse, Sync-Schlüssel und Freigabecode haben eine Schaltfläche 📋 Einfügen und „Einfügen“ per Rechtsklick (oder langem Drücken), unter Windows, Linux, Android und im Web, sodass ein anderswo kopierter Schlüssel direkt hineinkommt.

## 0.4.9 - 2026-11-28

### Verbessert
- Smartphones: Ziehe Haftnotizen am ≡-Griff in deine eigene Reihenfolge. Sie wird mit deinen anderen Smartphones abgeglichen (auf breiteren Bildschirmen platzierst du Notizen weiter frei auf der Pinnwand).
- Ziehst du eine Aufgabe oder Notiz an den oberen oder unteren Bildschirmrand, scrollt die Liste mit, sodass sie überallhin in einer langen Liste kann.
- Windows und Linux: Das RustDo-Symbol im Infobereich erscheint jetzt immer, wenn RustDo im Hintergrund läuft, auch wenn es nur für Erinnerungen da ist. Sein Menü hat „RustDo vollständig beenden“, das auch das Fenster schließt.
- Windows: Unter Einstellungen › Hintergrund gibt es „Auch erinnern, wenn RustDo geschlossen ist“. Schaltest du das und die Haftnotizen im Hintergrund aus, beendet das Schließen RustDo vollständig.
- Wenn du das Fenster zum ersten Mal schließt, während RustDo im Infobereich weiterläuft, sagt es dir das, mit einer Schaltfläche, um stattdessen vollständig zu beenden. Auch unter Einstellungen › Hintergrund gibt es „RustDo vollständig beenden“.
- Version aus dem Microsoft Store: Solange das Fenster geschlossen ist, installiert RustDo ein wartendes Store-Update selbst und startet danach wieder, statt das Update im Infobereich aufzuhalten.
- Der ≡-Griff zum Ziehen von Aufgaben erscheint auf Smartphones und Tablets jetzt bei jeder Sortierung; beim Verschieben wechselt die Liste von selbst zu „Meine Reihenfolge“.
- Nach dem Hinzufügen einer Aufgabe öffnet sich ihr Feld „Unteraufgabe hinzufügen“ direkt darunter (nicht oben in der Liste). Fügst du eine weitere Aufgabe hinzu, wandert das Feld dorthin; ✖ schließt es.

### Behoben
- Die Seite einer Liste zeigt jetzt dasselbe Symbol wie im Menü (🏠 für Aufgaben, 👥 für geteilte Listen, 📋 für eigene Listen, 📚 für Alle) statt ☰, das wie die Menü-Schaltfläche aussah.
- Smartphones und Tablets: Eine Aufgabe oben in der Liste am ≡-Griff nach unten zu ziehen, löste „Zum Aktualisieren ziehen“ aus, statt die Aufgabe zu verschieben.

## 0.4.8 - 2026-11-27

### Behoben
- Die Seite „Haftnotizen“ beginnt jetzt wie alle anderen Seiten: ☰ (auf Smartphones), der Titel, dann „+ Neue Notiz“ und ein Menü „Mehr“ (sichern, wiederherstellen, aufräumen), alles in einer Zeile.
- Smartphones und Tablets: Aufgaben lassen sich jetzt in die eigene Reihenfolge ziehen. In „Meine Reihenfolge“ hat jede Aufgabe einen ≡-Griff: Ziehe ihn nach oben oder unten (Ziehen an anderer Stelle scrollt weiterhin die Liste).
- Smartphones und Tablets: Auch Listen lassen sich jetzt neu anordnen, mit dem ≡-Griff an jeder Liste im Menü.
- Haftnotizen haben auf Smartphones und Tablets eine größere Ecke zum Ändern der Größe mit dem Finger.

## 0.4.7 - 2026-11-27

### Neu
- Aufgaben in eigener Reihenfolge: Ziehe eine Aufgabe nach oben oder unten (oder Rechtsklick bzw. langes Drücken › Nach oben, Nach unten, Ganz nach oben). Die Sortierung wechselt zu „Meine Reihenfolge“, die mit deinen anderen Geräten und geteilten Listen abgeglichen wird.
- Haftnotizen haben auf Smartphones und Tablets größere + / 🎨 / 🗑-Schaltflächen, die sich leicht antippen lassen.
- RustDo für Linux auf ARM64 (Raspberry-Pi-Desktops, ARM-Laptops): .deb, .rpm, AppImage und .tar.gz.
- RustDo Hub für Linux ohne Docker: fertige Downloads für PCs (x86_64) und Raspberry Pi (arm64) sowie ein Installationsskript, das den Hub als Dienst einrichtet, der mit dem Computer startet.
- RustDo für Linux gibt es jetzt auch als .rpm (Fedora, openSUSE) und als .tar.gz für jede Distribution, mit einem Installationsskript, das RustDo ins App-Menü aufnimmt.
- Schritt-für-Schritt-Einrichtung von RustDo Hub auf einem Synology-NAS (Container Manager), in der Hub-Anleitung und in RustDos „So funktionieren Synchronisieren und Teilen“.

### Behoben
- Web-App auf Android-Smartphones und -Tablets: Buchstaben konnten ungleichmäßig wirken, manche abgeschnitten oder verschoben (der Grafikchip zeichnete Text zu ungenau). Text wird jetzt mit voller Genauigkeit gezeichnet und ist so scharf wie in der Android-App.
- Android: Unter Einstellungen › Hintergrund fehlte die Schaltfläche „+ Bild…“, sodass keine eigenen Fotos möglich waren. Sie ist jetzt da und öffnet die Fotoauswahl von RustDo.
- Text ist über Hintergrundbildern besser lesbar: Datum, Hinweise und anderer heller Text verschwimmen nicht mehr mit dem Bild, im dunklen wie im hellen Design.

## 0.4.6 - 2026-11-27

### Neu
- Hintergrundbilder: Wähle unter Einstellungen › Hintergrund eines für die ganze App, und gib jeder Liste ihr eigenes (Rechtsklick oder langes Drücken auf die Liste › Hintergrund…). Wähle aus eingebauten Hintergründen oder deinen eigenen Bildern (unter Android zeigt RustDo deine Fotos, sobald du es erlaubst). „Hintergrund abdunkeln“ hält die Aufgaben gut lesbar.
- Hintergründe werden mit „Meine Geräte synchronisieren“ abgeglichen, auch deine eigenen Bilder: Ein am PC gewähltes Bild erscheint auch auf dem Smartphone. (Dafür braucht dein RustDo Hub Version 0.4.6 oder neuer.)
- Auch Haftnotizen können ein Bild haben: Öffne das 🎨-Menü einer Notiz › Bild und wähle ein eingebautes oder ein eigenes. Die Farbe der Notiz tönt es, damit der Text gut lesbar bleibt.
- „RustDo für deine anderen Geräte“ (im ⚙-Menü) zeigt alle Wege zu RustDo: Microsoft Store, Windows-Installer, Android, Linux, die Web-App und den Hub.
- Unten in den Listen eine Schaltfläche „Tritt der RustDo-Discord-Community bei!“ für Ideen, Fehler und Hilfe.
- RustDo Hub kann als Home-Assistant-App laufen: Füge im App-Store von Home Assistant das RustDo-Repository hinzu und installiere RustDo Hub. Mehr in der Hub-Anleitung.

### Behoben
- Web-App auf Smartphones und Tablets: Auf manchen Bildschirmen wirkten die Buchstaben gestaucht. „Scharfe Schrift“ (standardmäßig an, im ⚙-Menü) wirkt jetzt auch dort, wie in der Android-App.
- Das Docker-Image von RustDo Hub funktioniert jetzt auch, wenn sein Datenordner root gehört (häufig bei NAS-Geräten und in Home Assistant).

## 0.4.5 - 2026-11-26

### Behoben
- Android: Nachdem die Bildschirmtastatur ausgeblendet wurde (mit ihrer Ausblenden-Taste, der Zurück-Geste oder durch einen App-Wechsel), brachte ein Tippen auf ein Textfeld sie nicht zurück. Jetzt öffnet ein Tippen auf ein Feld immer die Tastatur, und sie kommt wieder, wenn du zu RustDo zurückkehrst.
- „Aufgabe hinzufügen“ zeigt eine Schaltfläche „Hinzufügen“, sobald du tippst, sodass eine Aufgabe auch ohne die Eingabetaste gespeichert werden kann.

## 0.4.4 - 2026-11-26

### Neu
- Liste anzeigen: eine große, gut lesbare Seite mit den Aufgaben einer Liste und ihren Notizen, zum Beispiel eine Medikamentenliste für den Arzt. Kopiere sie als Text für eine E-Mail oder Nachricht, oder drucke sie am Computer bzw. speichere sie als PDF. Du findest es neben dem Namen der Liste oder per Rechtsklick (langem Drücken) auf eine Liste.

### Verbessert
- Smartphones: Eine deutliche Zurück-Schaltfläche (ein Pfeil in einer runden Schaltfläche) ersetzt das kleine „<“ im Menü, in den Einstellungen, in den Aufgabendetails und bei den Haftnotizen.
- Android: Die Zurück-Taste und die Zurück-Geste funktionieren jetzt in RustDo. Zurück schließt, was offen ist, geht zur Liste und zu „Mein Tag“ zurück und verlässt von „Mein Tag“ aus die App.
- Listen neu anordnen: Ziehe eine Liste im Menü nach oben oder unten, oder halte sie auf dem Smartphone gedrückt und wähle „Nach oben“ oder „Nach unten“. Die neue Reihenfolge wird mit deinen anderen Geräten synchronisiert.
- Neue Installationen auf Deutsch, Spanisch oder Französisch starten mit dem 24-Stunden-Format, und der Name der Standardliste wechselt mit, wenn du die Sprache änderst.
- „Neuigkeiten“ erscheinen jetzt in deiner Sprache (Deutsch, Spanisch oder Französisch).
- RustDo Hub speichert jeden Tag eine Kopie seiner Daten (die letzten 14 Tage) in einem Ordner „backups“ neben hub.json.

## 0.4.3 - 2026-11-26

### Neu
- RustDo spricht jetzt auch Spanisch und Französisch: Unter Einstellungen › Sprache gibt es Español und Français neben English und Deutsch, und „Automatisch“ wählt sie, wenn dein Gerät sie nutzt. Daten erscheinen im jeweiligen Format, z. B. „Miércoles, 25 de noviembre“ und „Mercredi 25 novembre“.

## 0.4.2 - 2026-11-25

### Neu
- Direkt aus Microsoft To Do importieren: Wähle im Menü „Aus To Do importieren“ und dann „Mit Microsoft anmelden“. RustDo liest alle deine Listen, Aufgaben, Schritte, Notizen, Fälligkeitsdaten und Erinnerungen und zeigt, was importiert wird. Deine To Do-Daten werden nur gelesen, nie geändert. Funktioniert unter Windows, Linux, Android und in der Web-App, mit privaten sowie Geschäfts- oder Schulkonten von Microsoft.
- Auch Android kann jetzt aus Microsoft To Do importieren (per Anmeldung).

### Behoben
- Deutsch: Einige längere Meldungen, die noch auf Englisch erschienen, sind jetzt übersetzt.

## 0.4.1 - 2026-11-25

### Neu
- RustDo spricht Deutsch: Unter Einstellungen › Sprache gibt es Automatisch, English und Deutsch. „Automatisch“ folgt der Sprache deines Geräts. Daten erscheinen im deutschen Format, z. B. „Mittwoch, 25. November“.
- Die Sprachunterstützung ist jetzt eingebaut, sodass weitere Sprachen folgen können.

## 0.4.0 - 2026-11-25

### Neu
- Meine Geräte synchronisieren: Halte deine Listen, Aufgaben, Haftnotizen und Einstellungen (Design, Uhrzeitformat, Sortierung) auf all deinen Geräten gleich, auch in der Web-App, über deinen eigenen RustDo Hub. Schalte es in den Einstellungen unter „Meine Geräte synchronisieren“ ein: Dein erstes Gerät bekommt einen Sync-Schlüssel, deine anderen Geräte nutzen dieselbe Hub-Adresse und denselben Schlüssel. Jedes Gerät behält seine eigenen Erinnerungsfenster und die Position seiner Haftnotizen.
- Listen, die du mit der Familie teilst, bleiben von deiner eigenen Synchronisierung getrennt: Die anderen sehen nur die geteilte Liste, und deine anderen Geräte treten ihr von selbst bei.
- Für „Meine Geräte synchronisieren“ wird RustDo Hub 0.4.0 benötigt (aktualisiere das Docker-Image oder den Windows-Hub). Damit kann sich auch die Web-App verbinden, wenn der Hub eine https://-Adresse hat.
- Die Hub-Anleitung in den Einstellungen erklärt jetzt neben dem Teilen von Listen auch das Synchronisieren deiner eigenen Geräte.
