# osdoctor

**Linux desktop health checks for Arch/Hyprland systems.**

`osdoctor` is a small, read-only command-line utility written in Rust that
diagnoses common problems on Arch Linux desktops — especially Hyprland and
Omarchy-style setups. It inspects the system, the package manager, systemd
services, and your desktop session, then prints a clean, grouped report (or
JSON) with a meaningful exit code you can use in scripts.

[![CI](https://github.com/pablofc18/osdoctor/actions/workflows/ci.yml/badge.svg)](https://github.com/pablofc18/osdoctor/actions/workflows/ci.yml)
![Rust](https://img.shields.io/badge/Rust-2021-orange)
![License: MIT](https://img.shields.io/badge/license-MIT-green)

---

## Example

```console
$ osdoctor scan
System
  ✓ Kernel detected: 7.0.9-arch2-1
      Linux 7.0.9-arch2-1 #1 SMP PREEMPT_DYNAMIC Fri, 22 May 2026 19:25:09 +0000
  ✓ Root filesystem has 403 GB free
  ✓ /tmp is writable
  ✓ Memory: 26792 MB available of 31956 MB

Services
  ✓ No failed system services
  ✓ No failed user services

Packages
  ✓ pacman database is unlocked
  ✓ pacman log exists
  ✓ No orphan packages

Desktop
  ✓ Wayland session detected
  ✓ Hyprland socket detected
  ! Hyprland bind references missing command: ghostty
  ✗ Waybar config references missing script: ~/.config/waybar/scripts/battery

Summary
  Status: FAIL
  Checks: 11 ok, 1 warning, 1 failure
```

Output is colorized when writing to a terminal, and plain when piped or when
`NO_COLOR` is set. See [`examples/`](examples/) for full text and JSON samples.

---

## Features

- **Zero configuration** — run `osdoctor scan` and get an answer.
- **Read-only and safe** — no system changes, no root required for normal scans.
- **Grouped checks** across four domains: System, Services, Packages, Desktop.
- **Four status levels:** `OK`, `WARN`, `FAIL`, `SKIP` (skipped when a check
  doesn't apply to your environment).
- **Script-friendly:** stable exit codes and `--json` output.
- **`--strict` mode** to treat warnings as failures in CI.
- **Small and dependency-light:** Rust std + POSIX, with `libc` the only crate
  (used for `uname`, `statvfs`, `glob`, and `access`).

### Checks

| Group    | ID                        | What it checks                                                        |
|----------|---------------------------|-----------------------------------------------------------------------|
| System   | `kernel-version`          | Kernel release via `uname(2)`                                         |
| System   | `root-disk-space`         | Free space on `/` (WARN < 10 GB, FAIL < 2 GB)                         |
| System   | `tmp-writable`            | `/tmp` is writable                                                   |
| System   | `memory-info`             | `/proc/meminfo` (WARN when < 10% available)                          |
| Services | `failed-system-services`  | Failed system units — sd-bus or `systemctl --failed`                 |
| Services | `failed-user-services`    | Failed user units — sd-bus or `systemctl --user --failed` (SKIP without a user session) |
| Packages | `pacman-lock`             | `/var/lib/pacman/db.lck` present → FAIL                              |
| Packages | `pacman-log`              | `/var/log/pacman.log` exists and is readable                         |
| Packages | `orphan-packages`         | Orphans via libalpm or `pacman -Qdtq` (SKIP without pacman)          |
| Desktop  | `wayland-session`         | `XDG_SESSION_TYPE=wayland`                                            |
| Desktop  | `hyprland-socket`         | Hyprland IPC socket exists (SKIP when not running Hyprland)          |
| Desktop  | `hyprland-binds`          | `exec` bind targets resolve in `PATH` (follows `source =` includes)  |
| Desktop  | `waybar-scripts`          | Local scripts referenced by the Waybar config exist                  |

---

## Installation

### From source

Requires a Rust toolchain (stable, **Rust ≥ 1.70**) with `cargo`.

```sh
cargo build --release
sudo install -Dm755 target/release/osdoctor /usr/local/bin/osdoctor
sudo install -Dm644 man/osdoctor.1 /usr/local/share/man/man1/osdoctor.1
```

### Backends

The service and package checks shell out to `systemctl` and `pacman`. The
optional direct-library backends (libsystemd/sd-bus and libalpm) the C build
offered are not yet ported; behaviour on a normal Arch system is identical,
since those were faster paths for the same data.

### Arch Linux (PKGBUILD)

A [`packaging/PKGBUILD`](packaging/PKGBUILD) is included. To build a package
from a tagged release:

```sh
cd packaging
makepkg -si
```

(For a real release, update `sha256sums` with `updpkgsums`.)

---

## Usage

```sh
osdoctor scan            # run all checks
osdoctor scan --json     # machine-readable output
osdoctor scan --strict   # warnings count as failures for the exit code
osdoctor system          # only system checks
osdoctor services        # only service checks
osdoctor packages        # only package checks
osdoctor desktop         # only desktop/session checks
osdoctor version
osdoctor help
```

---

## Exit codes

| Code | Meaning                                                        |
|------|----------------------------------------------------------------|
| `0`  | All checks passed (no warnings, no failures)                   |
| `1`  | One or more warnings, no failures                              |
| `2`  | One or more failures (or any warning under `--strict`)         |
| `3`  | Invalid usage or an internal/tooling error                     |

Example use in a script:

```sh
if ! osdoctor scan --strict --json > /tmp/health.json; then
    echo "System health degraded" >&2
fi
```

---

## JSON output schema

`osdoctor <command> --json` prints a single object:

```json
{
  "status": "fail",
  "summary": { "ok": 11, "warn": 1, "fail": 1, "skip": 0 },
  "groups": [
    {
      "name": "System",
      "checks": [
        {
          "id": "root-disk-space",
          "status": "ok",
          "message": "Root filesystem has 403 GB free"
        }
      ]
    }
  ]
}
```

- `status` — overall status, the worst present (`fail` > `warn` > `ok`).
  Not affected by `--strict` (only the exit code is).
- `summary` — counts per status level.
- `groups[].name` — the group label (`System`, `Services`, `Packages`, `Desktop`).
- `groups[].checks[]` — individual results:
  - `id` — stable machine-readable identifier (see the checks table).
  - `status` — `ok` | `warn` | `fail` | `skip`.
  - `message` — short human-readable summary.
  - `detail` — optional extra context; only present when non-empty.

A complete sample is in [`examples/sample-output.json`](examples/sample-output.json).

---

## Development

```sh
cargo build            # build (debug)
cargo build --release  # optimized build
cargo test             # run unit + integration tests
cargo fmt              # format
cargo clippy           # lint
```

The code is a small multi-module Rust crate:

```
src/model.rs   core result/summary types
src/cli.rs     argument parsing and dispatch
src/output/    text and JSON renderers
src/checks/    one module per check group (system, services, packages, desktop)
src/util/      exec, fs, and string helpers
tests/         integration tests (JSON golden + CLI)
```

`libc` is the only dependency, used for the handful of syscalls std does not
expose (`uname`, `statvfs`, `glob`, `access`).

---

## Roadmap

Planned work is tracked in the [issue tracker](https://github.com/pablofc18/osdoctor/issues):

- [Use `libalpm`/sd-bus direct backends](https://github.com/pablofc18/osdoctor/issues/3) — available in the C build on `main`; not yet ported to Rust.
- [Config file for thresholds and toggling checks](https://github.com/pablofc18/osdoctor/issues/4)
- [Plugin-style external checks](https://github.com/pablofc18/osdoctor/issues/5)
- [TUI mode](https://github.com/pablofc18/osdoctor/issues/6)
- [HTML report exporter](https://github.com/pablofc18/osdoctor/issues/7)
- [Opt-in fix suggestions](https://github.com/pablofc18/osdoctor/issues/8) — still read-only by default.
- [AUR package](https://github.com/pablofc18/osdoctor/issues/9)

---

## Safety

- **Read-only by design.** osdoctor inspects state; it never modifies it.
- **No destructive actions** and no automatic "repairs".
- **No root required** for normal scans.
- Some checks **skip** themselves cleanly when they don't apply (e.g. no
  Hyprland session, no pacman, no systemd user bus).
- Commands are run with fixed, hard-coded argument lists — no user-controlled
  input is ever passed to a shell.

---

## License

[MIT](LICENSE) © 2026 pablofc18
