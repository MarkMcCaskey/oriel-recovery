# ORIEL-1: build and model notes

Run the CLI with `cargo run --release --offline --bin ground`. Build the
static browser player with `./scripts/build-web.sh`, then serve `dist` with
`python3 -m http.server 8080 --directory dist` and open http://localhost:8080.
The browser build needs the Rust wasm32-unknown-unknown standard library
(`rustup target add wasm32-unknown-unknown`). Python 3 is only a packaging
and local-server convenience. Neither player has an external runtime service.

Native packaging: `./scripts/build-player.sh`. The resulting
`dist/player/ground` has its engineering archive and scenario embedded and
can run by itself. Browser assets can be hosted on any static HTTP server,
including GitHub Pages under a repository subpath. After the first complete
load on localhost or HTTPS, the service worker caches the browser player
for offline reloads. An already open page needs no network to simulate.

## Implemented model

One dependency-free Rust core runs natively and as WebAssembly. JavaScript
provides the workstation interface and IndexedDB persistence. Simulation
runs in a worker so advancing virtual time does not block browser controls.
The ISA, board, firmware binary, memory and registers are real within the
fictional machine. The assembler emits the bytes that the CPU executes.
Telecommands and telemetry are implemented by flight machine code. The
host does not recognize a repair payload or set a victory flag.

Circular two-body orbit: radius 7051 km, inclination 97.6 degrees. A rotating
spherical Earth has radius 6371 km and sidereal period 86164.0905 seconds.
The simulated ground station is at 51.5 degrees north, 7.4 degrees east,
with a 10 degree elevation mask. This is an idealized orbit, not a TLE/SGP4
prediction. No real station or spacecraft is controlled. Atmospheric drag,
J2 precession, Doppler tracking, terrain and antenna steering are omitted.
Contact boundaries are resolved to 1 ms. Uplink/downlink accounting includes
all frame bytes and the documented fixed preamble overhead.

The L32 CPU is a deliberately small fictional 32-bit architecture, not an
ARM emulator. It models instruction execution, interrupts, timer, watchdog,
RAM, executable flash, persistent recorder memory and a communications FIFO.
Its effective instruction rate is 20 kHz and devices use a 1 ms scheduling
quantum. Generic hardware faults act on bus accesses. NOR programming and
interrupted writes have explicit semantics in the hardware reference.
Sensors are deterministic functional signals, not a thermal, electrical,
attitude-control or radiation-environment simulation. This simulator focuses
on software recovery engineering rather than full spacecraft physics.

The local bench loads the archived linked firmware on nominal hardware.
It supports memory inspection, code loading, single stepping, PC breakpoints,
and configurable fault hypotheses. Bench debug stepping freezes peripheral
time; running resumes it. The bench has no connection to flight state.

## Persistence and limits

The CLI autosaves to `.local/station.session` after each completed command;
use `--state PATH` for another campaign. Use `--hardcore` for a campaign
without snapshot/restore/restart commands. In the browser these choices
are under New campaign. Exported session files work in either player.
Hardcore still autosaves and resumes. It is an honor-system feature on your
own computer; a wholly offline game cannot prevent deliberate save editing.

Normal mode supports eight named checkpoints. The station stores at most
4096 traffic entries, 2048 received packets, 4096 pending uplink frames and
128 workspace files (1 MiB each, 4 MiB total). Export research notes and
transcripts when preserving a complete investigation. Browser storage can
be cleared by the browser or OS; use Export session for portable backups.
Autosave failures are shown as EXPORT TO SAVE in the session panel.

The scenario and saves are opaque checksummed containers for accidental
spoiler avoidance, not cryptographic protection. Deliberately reverse
engineering a completely offline executable or its data can expose internals.
The public source includes the general emulator and tools. Authoring data,
reference procedures and privileged campaign validation are excluded from
Git and from both player packages. No cloud secret is used to hide them.

## Verification and builds

`cargo test --offline` runs the public machine, communication, archive,
console and persistence tests. `./scripts/build-web.sh` creates the browser
build; `node scripts/test-web.mjs` exercises the compiled WebAssembly module.
`python3 scripts/audit.py` checks the tracked/publication file list.
The developer workspace additionally contains isolated campaign validation;
it is deliberately unavailable in the public player checkout.

The repository uses only the Rust standard library, browser platform APIs,
and Python standard library for packaging. There are no Cargo, npm or CDN
runtime dependencies. Rust 1.93.0 was used for release verification.
QEMU and Unicorn were evaluated as established alternatives, but their
broader native emulation integration would add substantial complexity to a
single offline native/browser core. The smaller specified ISA preserves
the memory, executable-patching and debugging problems central to this game.

Inspiration and reference: PistonMiner, “Hacking yourself a satellite –
recovering BEESAT-1”, 38C3 (2024):
https://media.ccc.de/v/38c3-hacking-yourself-a-satellite-recovering-beesat-1
The fictional incident and mission artifacts are original. The reference
is historical context, not a recovery guide for this scenario.
Library references: https://www.unicorn-engine.org/ and
https://www.qemu.org/docs/master/system/target-arm.html
