# Unit systemd (user) per la telemetria di Exo

Installazione:
    cp deploy/systemd/* ~/.config/systemd/user/
    systemctl --user daemon-reload
    systemctl --user enable --now exo-listen.service exo-watchdog.timer exo-concept-watch.service

Cosa fanno (tutti e tre, 07/10/2026):
- **exo-listen** — riceve la telemetria UDP di Exo (porta 42800) nel JSONL.
  `Restart=always`: se muore, rialza.
- **exo-watchdog.timer** ogni 2 min → `tools/exo_watchdog.sh`: controlla
  l'età del **LOG** (non del processo). >180 s di silenzio = inceppato →
  kill + restart. Nato perché il listener del 05/10 è rimasto vivo ma fermo
  per 48 h: dati persi in silenzio.
- **exo-concept-watch** → `tools/exo_concept_watch.py`: quando il `concept`
  esce da `riposo`, scarica l'antefatto (120 pacchetti ≈ 2 min) in
  `concept_events.{jsonl,log}` e manda un avviso Telegram via `avviso.sh`.

REGOLE (scritte nel codice, lette qui):
1. **Mai due listener sulla 42800** — `SO_REUSEADDR` li fa convivere e il
   kernel consegna ogni datagramma a UNO solo: il secondo ruba pacchetti
   senza fare rumore.
2. Il watchdog deve guardare i **dati**, non il processo: un processo vivo
   che non scrive non fa errore, non muore, e non si vede.
