# Cornerstone Stats

System stats, a task manager and a services manager in one bar widget, part
of the Cornerstone desktop by Kush Beaver Productionz. It started life as a
fork of OmaStats 1.8.1 by Carmine Paolino (MIT, see LICENSE) and was
rewritten from scratch in version 3; OmaStats was the reference for ideas.

## What it shows

In the bar, one readout per module (CPU, GPU, VRAM, memory, disks, network,
sensors, battery), each as a mini graph, a ring, a figure, or a picture and
a figure. Labels are stacked letters, the short name inline ("CPU 6%"), or a
glyph. Click a readout to open its tab, right-click for btop, middle-click
to refresh.

The panel's tabs:

- **Apps**: open apps grouped by the program that owns them, with that
  program's CPU and memory (child processes included). Bring here, Close,
  End task, Force kill. Below, every process, sortable by CPU, memory or
  disk and searchable.
- **CPU, GPU, Memory, Disks, Network, Sensors, Power**: history graphs,
  per-core rings, VRAM and board power, memory pressure, volumes with TRIM
  checks and one-click fixes, per-program disk and network traffic, every
  temperature and fan, where the power goes, the battery.
- **Services**: Yours (units in ~/.config/systemd/user), Desktop, System.
  Start, stop, restart, logs. System actions ask for your password.
- **Settings**: everything above, saved into the widget's entry in
  ~/.config/omarchy/shell.json.

Keys in the panel: Left/Right or h/l change tab, 1 to 9 jump to a tab, Up/Down
scroll, / searches processes, r refreshes, s opens Settings, Esc closes.

From a script: `omarchy-shell cornerstone.stats show apps` (or `toggle TAB`,
`hide`, `status`, `refresh`).

## How it is built

- `bin/collector` starts `bin/cornerstone-stats-collector-<arch>` (x86_64 and
  aarch64 are included). The collector is Rust (`collector/`), reads
  /proc and /sys directly, and streams one JSON line per tick; the format is
  in `collector/PROTOCOL.md`. It also runs the panel's actions:
  `summon ADDRESS`, `close PID`, `unit SCOPE ACTION UNIT`.
- `Service.qml` runs one collector for the whole shell and keeps the
  histories; `BarWidget.qml` is the bar strip and the panel.
- `lib/` holds the logic with no Qt in it (formatting, settings, reading a
  frame, theme colours); `ui/kit` the drawing pieces; `ui/pages` the tabs.
- End task uses `~/.local/bin/taskman-end` (part of cornerstone-desktop),
  which ends every copy of a program you own and refuses the desktop's own
  core processes.

## Building the collector

    cd collector
    cargo build --release
    cp target/release/cornerstone-stats-collector ../bin/cornerstone-stats-collector-x86_64

For ARM64 (static, runs on any aarch64 Linux):

    rustup target add aarch64-unknown-linux-musl
    cargo build --release --target aarch64-unknown-linux-musl
    cp target/aarch64-unknown-linux-musl/release/cornerstone-stats-collector ../bin/cornerstone-stats-collector-aarch64

## Tests

    node tests/lib.test.cjs        # formatting, settings, frame reading, colours
    python3 tests/qml_check.py     # QML parses, plain text, no bare Color
    (cd collector && cargo test)   # collector parsers
