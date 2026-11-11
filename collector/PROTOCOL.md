# Collector protocol

`cornerstone-stats-collector` is a long-running process started by
`Service.qml`. It writes one JSON object per line on stdout ("a frame") and
reads one command per line on stdin. It also has one-shot subcommands used by
the panel's buttons.

Everything here is our own contract: the QML side (`lib/*.js`, `Service.qml`)
reads exactly these names. Units are fixed per field and named below:
bytes, bytes per second, percent 0-100, degrees Celsius, MHz, watts, seconds.
A reading the machine does not offer is `null` (or the key is left out of an
object that is itself optional). Never send NaN or Infinity.

## Running

    cornerstone-stats-collector [--interval SECONDS]     stream frames
    cornerstone-stats-collector once                    print one full frame (tests)
    cornerstone-stats-collector summon ADDRESS          bring a Hyprland window to the current workspace and focus it
    cornerstone-stats-collector close PID               politely close every window of that process
    cornerstone-stats-collector unit SCOPE ACTION UNIT  start|stop|restart a systemd unit (SCOPE user|system; system via pkexec)

`bin/collector` (a POSIX sh launcher) picks the binary for the machine
(`bin/cornerstone-stats-collector-x86_64` or `-aarch64`) and execs it with the
same arguments. When no binary fits it prints one frame
`{"v":1,"seq":0,"problems":["..."]}` (only those three keys) that says how to build one, then sleeps,
so the panel shows the reason instead of restarting it in a loop.

## Commands on stdin

    detail 0|1|2        0 bar only, 1 a panel is open (top processes), 2 every process
    focus PAGE          page the panel shows: apps, cpu, gpu, memory, disks, network,
                        sensors, power, services, settings, or none
    interval SECONDS    0.1 to 30
    publicip            look up the public address now (background thread)
    refresh             send the next frame at once and redo the page extras
                        (apps, services) instead of waiting for their own timer
    quit

Unknown or malformed lines are ignored. A command that changes what is
collected takes effect at once (the loop wakes), not after the interval.

## Frame

Top level, always present:

    v          1
    seq        frame number, from 1
    interval   seconds between frames now
    problems   [string]  short plain sentences for the status line, usually []

### cpu (always)

    model        "AMD Ryzen 9 5950X 16-Core Processor"
    cores        physical cores
    threads      logical CPUs
    usage        { total, user, system, iowait }   percent of the whole machine
    perThread    [percent]  one per logical CPU, in CPU order
    efficiency   [index]    logical CPUs that are efficiency cores (hybrid Intel), else []
    mhz          average current clock
    temp         package temperature (k10temp Tctl/Tdie, coretemp package, zenpower, cpu_thermal)
    watts        package power: RAPL, else zenpower's core + SoC rails, else an APU's hwmon, else null
    load         [1m, 5m, 15m]
    uptime       seconds

### memory (always)

    total, used, available, free, apps, cached, shared     bytes
    swapTotal, swapUsed                                    bytes
    zram         { stored, original } bytes, or null when no zram/zswap holds anything
    stall        { some, full }  /proc/pressure/memory avg10, percent

`used = total - available` (the headline figure). The breakdown partitions
the total: `cached = Cached + Buffers + SReclaimable - Shmem`, `shared = Shmem`,
`free = MemFree`, `apps = total - free - cached - shared` (programs and the
kernel), so `apps + shared + cached + free = total` and a ring of them closes.
`zram.stored` is RAM actually used (zram mem_used_total + zswap pool),
`zram.original` the data before compression.

### gpus (always, [] when none)

    id           PCI address "0000:0c:00.0" (or a stable name when there is no PCI device)
    vendor       "amd" | "nvidia" | "intel" | "other"
    name         marketing name ("Radeon RX 7900 XT/7900 XTX/7900 GRE/7900M")
    integrated   bool
    asleep       bool   runtime-suspended; then load/mhz/temp/watts are null and the card is not woken
    load         percent or null (i915/xe report none)
    mhz, memMhz  MHz or null
    temp         edge temperature or null
    hotspot      junction temperature or null
    watts        board power (amdgpu power1_average or power1_input, nvidia-smi) or null
    wattsCap     power limit or null
    fanPercent   percent or null
    fanRpm       rpm or null
    vramUsed, vramTotal   bytes or null (for an integrated card: GTT used/total)
    procs        only while focus is gpu, from DRM fdinfo: [{ pid, name, count, load, vram }]
                 busiest first, at most 12; the key is absent on the first gpu frame (load
                 needs two readings). vram = dedicated memory, or for an integrated card the
                 system memory its clients hold. NVIDIA's proprietary driver may not expose
                 fdinfo; its processes are then missing.

amdgpu cards must not be kept awake by reading them: when runtime PM may
suspend a card (power/control == "auto" and it is active), read it at most once
per autosuspend delay plus a margin and repeat the last values in between.

### disks (always)

    read, write        bytes/s over all physical drives
    stall              { some, full }  /proc/pressure/io avg10, percent
    drives             { kernelName: { id, model, read, write, busy, latency, queue, temp, rotational, removable } }
                       id = stable /dev/disk/by-id name (model + serial); busy percent of the interval,
                       latency ms per request or null when idle, queue average in flight
    volumes            [{ key, mount, device, drive, fstype, kind, size, used, avail, model, temp,
                          rotational, removable, checks }]
                       kind "internal" | "external" | "network"; drive = kernel name of the physical
                       disk under any LUKS/LVM/partition layers ("" for network); key = drive, or for a
                       network mount the source ("server:/share"). size/used/avail bytes.
                       Pseudo filesystems, snaps, bind mounts of the same device and subvolume
                       duplicates are left out (one entry per device, the shortest mount path).
                       Network shares are measured every 10 s in a forked child with a 1.5 s
                       deadline: a dead share (FUSE blocks even exit) gets null sizes, is left
                       alone for 30 s, and never has two children waiting on it. Local volumes
                       every 2 s. Per-process file walks use cached attributes
                       (AT_STATX_DONT_SYNC) so they never ask a network server either.
    checks             [{ id, fix, args, layer?, flags? }]
                       id "trim-blocked": SSD under dm-crypt opened without allow-discards.
                           layer = mapper name; fix "allow-discards", args [mapper name] when plain
                           (`cryptsetup refresh` takes the name).
                           Not reported together with trim-off for the same volume.
                       id "trim-off": SSD mounted without discard while fstrim.timer is not enabled.
                           fix "enable-fstrim", args [].
                       id "fat-flush": vfat/exfat mounted with flush or sync. flags [options];
                           fix "udisks-no-flush", args [uuid, device] when udisks mounted it, else null.

Volumes, drive ids and checks are refreshed every 10 s or when
/proc/self/mountinfo changes; rates every frame.

### network (always)

    rx, tx         bytes/s over real interfaces (not lo, not virtual bridges/veth/docker)
    online         bool, a default route exists
    defaultIface   name or ""
    publicIp       "" until looked up (only after a publicip command)
    ifaces         [{ name, up, isDefault, kind, ssid, signal, bitrate, speed, ipv4, ipv6,
                      rx, tx, rxTotal, txTotal }]
                   kind "ethernet" | "wifi" | "vpn" | "other"; signal dBm or null; bitrate Mb/s or null
                   (wifi); speed Mb/s or null (wired); ipv4/ipv6 [string] (no link-local v6)
    procs          only while focus is network (key absent otherwise):
                   [{ pid, name, count, rx, tx, connections }]
                   TCP only: sock_diag's tcp_info has bytes_received/bytes_acked per socket, UDP
                   sockets carry no byte counters, so UDP traffic (QUIC, games, voice) is missing.
                   Programs the user can read; rx/tx bytes/s, connections = established sockets;
                   programs with connections but no traffic are listed too. Busiest first, at
                   most 40; null until two readings exist (about a second).

### sensors (always)

    temps     [{ id, group, label, celsius, critical }]
    fans      [{ id, group, label, rpm, percent }]   rpm or percent may be null

`id` is stable across reboots: "<driver>@<device>/<input>" when the driver can
appear more than once (nvme, drivetemp, spd5118, jc42, ee1004, amdgpu, nouveau,
i915, xe) or does on this machine, else "<driver>/<input>". <device> is the PCI
address (for NVMe the controller's), else the device's sysfs name. Examples
here: "k10temp/temp1", "amdgpu@0000:0c:00.0/temp2", "nvme@0000:03:00.0/temp1".
NVIDIA cards read through nvidia-smi: "nvidia@<pci>/temp" and "nvidia@<pci>/fan".
`group` is a readable chip name: CPU, GPU, NVMe, Drive, Memory, Board, Wi-Fi,
Ethernet, ACPI, or the driver name. `label` is the sensor's own label made
readable ("Tctl", "edge", "Composite", "Fan 1"). An unlabeled temperature is
"" when it is its chip's only one (show the group alone: "Wi-Fi") and its
number ("1", "2") when there are several ("ACPI 1"). Unlabeled fans are "Fan N".
`critical` is the chip's crit (else max) limit or null. NVIDIA cards without hwmon
appear here too, from nvidia-smi.

### power (always)

    rapl      { package, core, uncore, dram, psys }  watts, each null when unreadable
    psu       watts from a PSU hwmon (corsairpsu, nzxt etc.) or null
    raplHint  true when RAPL exists but is unreadable without root

### battery (always; null on a desktop with nothing)

    present, percent, status ("Charging" | "Discharging" | "Full" | "Not charging" | "Unknown"),
    acOnline, watts, timeToEmpty, timeToFull (minutes or null), energyNow, energyFull,
    energyDesign (Wh), health (percent of design), cycles, voltage (V), model, technology,
    peripherals [{ name, percent, status }]   (mice, headsets: power_supply scope Device)

`present` is false and the main-battery fields null when only peripherals exist.

### procs (detail >= 1, else null)

A process row is a group: every process of one user with the same display
name ("chrome x17"), represented by the pid of its oldest member.

    { pid, name, count, cpu, mem, read, write }
    cpu percent of the whole machine (100 = every logical CPU busy)
    mem resident bytes summed over the group
    read, write bytes/s from /proc/<pid>/io (own processes only; others 0)

    count      number of user-space processes (kernel threads are neither counted nor listed)
    top        { cpu: [row], mem: [row], io: [row] }  six each
    all        [row] every group, only at detail 2, else null
    byDrive    only while focus is disks: { volumeKey: [row] } programs with files open on
               each volume (network shares too), by read+write then memory, at most 12 each;
               rows can have zero I/O. Absent until the first walk (about 2 s).

procs is null for the first ~250 ms after a panel opens: per-process figures
need two readings, so the second is taken at once and the frame follows it.

Display name: the kernel comm; when comm is 15 bytes (truncated), the first
word of argv[0]'s basename if it starts with comm. Never the full command line.

### apps (focus apps, else null)

Windows from Hyprland (its request socket, no hyprctl process) grouped by the
process that owns them. Refreshed every 2 s and on `refresh`.

    [{ pid, comm, exe, deleted, procs, cpu, mem, focused,
       windows: [{ address, class, title, workspace, hidden, focused }] }]

When the shell started us with a cleared environment the socket is found under
/run/user/<uid>/hypr/ (the live instance). cpu: percent of the whole machine used
by the process and all its descendants over the last process reading; mem: resident bytes of that tree. `deleted` true when the
executable was replaced on disk (updated or uninstalled while running). Sorted
by mem, at most 60.

### services (focus services, else null)

    { user: [unit], system: [unit] }
    unit: { unit, name, description, active, sub, enabled, mem, restarts, mine, scope }

`mine` is true for user units whose fragment lives in ~/.config/systemd/user.
Refreshed every 3 s on a background thread and on `refresh`, never blocking frames.
Units with load state not-found are left out.

## Cost

The stream must stay light: it runs all day. With the panel closed (detail 0)
it reads only what the bar can show, keeps procfs/sysfs files open and rereads
them with pread, and allocates little per frame. Target: well under 0.3% of one
core at 1 s (the previous collector's figure on the same machine), measured with
`tools/cpucost`.

Measured 2026-11-11 on the 5950X (60 s each, side by side, machine busy):
panel closed 0.12% (old sampler 0.27%); panel open on Apps with every process
1.18% (old sampler 1.28% plus 0.44% for its Python Apps helper); every process
on another page 1.20%. Resident memory about 3-4 MiB.

Build: `cargo build --release --manifest-path collector/Cargo.toml` (set
CARGO_TARGET_DIR outside the plugin folder: the shell reloads on file changes
under it). ARM64: `rustup target add aarch64-unknown-linux-musl`, then
`RUSTFLAGS="-C linker=rust-lld -C target-feature=+crt-static" cargo build --release --target aarch64-unknown-linux-musl`.
