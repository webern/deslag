
![Tracktor.it-Logo](web/assets/images/traecktor_logo_300px.png)
# tracktor.it!

Tracktor.it ist ein DSGVO-Beschwerdegenerator für ein Internet ohne Tracking.

Melde Webseiten und Apps mit Werbetrackern einfach & schnell und hilf dadurch, das Internet trackingfrei für alle zu machen. 

Webseite: [tracktor.it](https://tracktor.it/)

![Screenshot](screenshot.png)


# Ordner-Struktur

| Ordner | Beschreibung |
| ------ | ------------ |
| `/build` | Dieser Ordner wird von `scripts/build.js` erstellt und enthält das Production Build |
| `/data` | Hier sind alle Daten gespeichert (Sprachdateien, Tracker, Templates, ...) |
| `/generator` | Der eigentliche Beschwerdegenerator (React-App) |
| `/web` | Der PHP-Teil der Webseite |
| `/scripts` | Enthält Skripte zur Automatisierung |  

Die `data`-Verzeichnisse in `/web/data` und `/generator/src/data` werden automatisch mit dem Skript `/data/build-data.js` befüllt.


# Entwicklung

1. Programme installieren (nachfolgender Code ist für Debian)
```
sudo apt install nodejs npm php php-sqlite3
```

2. Dieses Repository klonen
```
git clone https://codeberg.org/rufposten/tracktor.it
```

3. Benötigte Pakete installieren
```bash
# Für die Daten
cd tracktor.it/data
npm install

# Für die React-App
cd tracktor.it/generator
npm install
```

4. Live-Entwicklungsumgebung starten  
Das Skript startet einen PHP-Server, den Webpack-Server und überwacht das Daten-Verzeichnis auf Änderungen und führt ggf. automatisch `data/build-data.js` aus.

```bash
# cd tracktor.it/scripts
chmod +x start-dev.sh
./start-dev.sh
```

5. Production Build erstellen
```bash
# cd tracktor.it/scripts
node build.js
# Production Build wird in Traecktor/build gespeichert
```

# Dokumentationen
- [Hilfe](https://tracktor.it/help.php)
- [Wiki: Neue Tracker einfügen](https://codeberg.org/rufposten/tracktor.it/wiki)
- [Admin-Panel](docs/admin-panel.md)

## Copyright und Lizenz
**Icons**  
[Fxemoji-Set](https://github.com/mozilla/fxemoji) von Mozilla unter [Creative Commons Attribution 4.0 International License](http://creativecommons.org/licenses/by/4.0/)

**tracktor.it!** is MIT licensed.
