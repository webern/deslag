# Testresultaten app 0.2.39 / firmware 0.2.33 (2026-09-14)

Dagelijkse efficiëntie met veel schermen: aanpassingen 1, 2, 4, 5 en 6 uit de
schaalcheck (incrementeel doorrekenen, historie gebundeld op de achtergrond, keepalive
als ping, één API-actie per bericht, rustige diagnostiek). Zie CHANGELOG 0.2.39 en
RELEASING "Compatibiliteit 0.2.39".

## Wat veranderde

- App: `HomeAssistant.dirty` + `Manager.sync_one(..., dirty)` bouwt alleen gewijzigde
  tegels en de bovenbalk als die er een toont; `Manager.screens()` (gecachte
  schermlijst uit alleen de ESPHome-schermentiteiten) vervangt `inventory()[0]` in de
  lus, de SSE-stream en de updater; `history_loop` + `refresh_histories` (één
  `recorder/statistics_during_period` per venster, REST-terugval, dirty-markering);
  `ping()` elke 120 s met `rev`, volledige herhaling per uur of bij een inbox-status
  uit `RESEND_STATES`; `HomeAssistant.send(..., action)` roept
  `esphome.<node>_screen_message` aan voor firmware 0.2.33+.
- Firmware: `runtime_tiles::layout_rev`, `op: ping`, herhaalde ongewijzigde layout
  antwoordt `Gesynchroniseerd`; API-actie `screen_message`; `Opgestart` (uptime als
  tijdstempel) vervangt `Uptime`; `debug` 300 s; instellingsgetallen publiceren alleen
  bij verandering vanuit `apply_screen_settings`.

## Geautomatiseerd

- Python (`.venv-portal`): 130 tests OK, waarvan 9 nieuw in `test_scaling.py`
  (encode/revisie/actienaam, bucketing, alleen dirty tegels herbouwd en verstuurd,
  bovenbalk volgt eigen entiteit, `dirty=None` stuurt verschillen, `force` alles,
  ping-inhoud, oude firmware zonder actie en met volledige keepalive, de echte
  `run()`-lus: eerste ronde, stille wake, ping na 120 s, hersturen na
  `Indeling opnieuw nodig` met guard, uurlijkse herhaling, één dirty tegel;
  schermcache alleen opnieuw bij register of schermdiagnose en met kopieën;
  statistiekbundel per venster, REST-terugval, dirty alleen bij gewijzigde waarden,
  cache opgeruimd; `HomeAssistant.statistics` met ms-tijdstempels, `mean`/`state`,
  lopende uur doorgetrokken).
- C++ (clang++ -std=c++17 -Wall -Wextra -Werror -I.): 13/13 PASS (ongewijzigd).
- `generate_packages.py --check` groen; bench-kopieën opnieuw gegenereerd.
- ESPHome 2026.6.2 compile: `easy-guition-device.yaml` SUCCESS (RAM 32,5%, flash
  18,6%), `easy-cyd-device.yaml` SUCCESS (RAM 31,5%, flash 77,4%); alleen de bekende
  enum-waarschuwing op `runtime_tiles.h:596`.

## Op de bordjes (USB, beide op de bench; HA 2026.9.1 op de Yellow)

- Firmware rechtstreeks via de ESPHome-API (`screen_message`-actie, beide borden):
  layout → `Indeling ontvangen`, states → `Tegels laden`/`Gesynchroniseerd`,
  dezelfde layout nogmaals → `Gesynchroniseerd` (geen wissel), ping met juiste
  revisie → `Gesynchroniseerd`, verkeerde revisie → `Indeling opnieuw nodig`, ping
  zonder `rev` → `Fout: ongeldig bericht`, bericht van 3 KB in één actie → verwerkt.
- Add-on 0.2.39 als lokale add-on op de Yellow (via de SMB-share en de Supervisor),
  GitHub-add-on 0.2.38 tijdelijk gestopt, de echte indelingen (CYD 8 tegels met drie
  sensortegels en weer, Guition 9 tegels) via ingress opgeslagen: beide schermen
  `Gesynchroniseerd`, Guition-render (`capture_ui.py`) toont Woonkamer met live
  waarden.
- Eerste poging met `Uptime` als `type: timestamp` onder dezelfde naam faalde in HA:
  "has a unit of measurement ... non-numeric device class: timestamp" (het register
  houdt eenheid `s` vast). Opgelost met de nieuwe entiteit `Opgestart`; na de flash
  toont die een tijdstempel en verwijdert HA `sensor.<scherm>_uptime` zelf.
- Herstart (USB-flash van beide borden terwijl de monitor liep): inbox
  `Klaar voor tegelconfiguratie` op t=41 s, volledige herzending op t=42 s (10
  berichten CYD, 11 Guition), `Gesynchroniseerd` binnen 1 s; daarna pings op
  t=168 s (beide), ertussen alleen de gewijzigde tegel `sensor.3d_printer_vermogen`.
  Alle verkeer als `esphome.*_screen_message`, nul `text.set_value`.
- Statistieken: twee van de drie CYD-sensoren hebben statistiek (23 uurrijen),
  `sensor.3d_printer_netfrequentie` is `unavailable` (geen rijen) en loopt via de
  REST-terugval; geen waarschuwingen in het add-onlog.

## Lange-termijntest (lokale add-on 0.2.39, beide schermen, HA 2026.9.1)

- 35 minuten (2100 s) gemonitord via de HA-websocket (`call_service` en
  `state_changed`): 100 berichten in totaal, 70 statusberichten (alleen de gewijzigde
  tegels, vooral `sensor.3d_printer_vermogen` op de CYD, dat elke paar seconden
  verandert, en lampen op de Guition) en 30 pings (15 per scherm); nul
  layout-herzendingen, nul wissels van de inbox-status, nul beschikbaarheidswissels;
  geen `text.set_value`. Add-oncontainer: 0,0% CPU, 41 MB. Schermdiagnostiek in HA:
  alleen heap (per 5 min) en wifi-signaal; de instellingsgetallen bleven stil.
- Gevonden en opgelost: na de registerverversing van de app (elke 10 min) werd alles
  herbouwd zonder verschil, wat de pingtimer resette; de ping kwam dan na ~210 s in
  plaats van ~120 s. Nu volgt `last` alleen een echte volledige verzending en `pinged`
  elk verzonden bericht (elk bericht ververst het feed-venster van het scherm).
  Regressietest in `test_scaling.py`. Add-on herbouwd en nog 700 s gemonitord: per
  scherm was de langste stilte 122 s (CYD) en 127 s (Guition), ruim binnen het
  venster van 300 s van de firmware.
- Guition-capture na de hele test: pagina 2/2 met live waarden (script "Koppel
  Speakers" Laatst 20:51, dezelfde minuut als de `media_player.join`-aanroepen in de
  monitor).
- Bekend, niet uit deze ronde: `weather.buienradar` ondersteunt geen uurverwachting;
  de `weather.get_forecasts`-aanroep daarvoor logt elke 30 minuten een ERROR in de
  HA-kernlog (gedrag sinds 0.2.23). `Opgestart` toonde op beide borden een
  tijdstempel dat ongeveer een minuut na het moment van ontvangst ligt (ESPHome's
  eigen berekening); stabiel, niet aangepast.
- Afsluiting: lokale add-on gestopt, GitHub-add-on 0.2.38 weer gestart (werkt ook
  met firmware 0.2.33: tekstblokjes en volledige herhaling). Max werkt de add-on in
  de winkel bij naar 0.2.39.
