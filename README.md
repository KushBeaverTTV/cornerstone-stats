# Cornerstone Stats (cornerstone.stats)

System stats, task manager and services panel, part of the Cornerstone desktop
by Kush Beaver Productionz. Built from OmaStats 1.8.1 by Carmine Paolino (MIT,
see LICENSE).

Tabs: Apps (task manager), CPU, GPU, Memory, Disks, Network, Sensors, Power,
Services, Settings.

- **Apps**: open apps grouped by the program behind them (CPU, memory of the
  whole program), with Bring here / Close / End task / Force kill; below it
  every process, sortable by CPU, memory or disk, searchable, each endable.
- **Services**: Yours (~/.config/systemd/user), Desktop, System. Start, stop,
  restart, logs. System actions ask for the password.
- Memory and CPU tabs keep the End buttons on their process lists.

Pieces: `bin/cornerstone-stats-sampler` (fast collector, from upstream; `sampler.py`
is its fallback), `kbstats-helper.py` (apps + services), `ui/AppsPage.qml`,
`ui/ServicesPage.qml`, `~/.local/bin/taskman-end` (End task).
Open a tab from a script: `omarchy-shell cornerstone.stats show apps`.
The original plugin is parked in ~/.local/share/plugin-backups/.
