# ORIEL-1 · Recovery station

A single-player satellite recovery engineering simulator, offline in your
terminal or browser. Investigate a fictional university CubeSat through a
bandwidth-limited ground station, incomplete engineering archives, and a
local debugging bench. Develop and execute real code on the simulated CPU.

## Launch the CLI

With Rust installed:

```sh
cargo run --release --offline --bin ground
```

Read the mission handover with `cat archive/operations.txt`. Type `help` for
the console reference. Your session is saved automatically. Use
`cargo run --release --offline --bin ground -- --hardcore --state .local/hardcore.session`
for a separate hardcore campaign.

## Launch in a browser

```sh
rustup target add wasm32-unknown-unknown
./scripts/build-web.sh
python3 -m http.server 8080 --directory dist
```

Open **http://localhost:8080**. The browser player runs the same Rust core
through WebAssembly. After its first complete load, it can reload offline
on localhost or HTTPS. Export sessions to keep backups or transfer to CLI.

## Mission briefing

> ORIEL-1 was launched in 2014. Its last accepted engineering telemetry is
> from 2016. Recent passes still show a carrier and intermittent command
> responses. The original flight team has dispersed. The archive is what
> survived.
>
> We don't know what's wrong with it. Here's everything we have.
> See if you can bring it back.

## Verification and packaging

```sh
cargo test --offline
./scripts/build-web.sh
node scripts/test-web.mjs
./scripts/build-player.sh
python3 scripts/audit.py
```

The standalone native player is `dist/player/ground`; the static website is
`dist/`. Neither requires a cloud service. The included GitHub Pages workflow
can publish the browser build when Pages is enabled for GitHub Actions.

The [console reference](docs/commands.md), [engineering archive](archive/manifest.txt)
and [build/model notes](docs/model.md) are spoiler-free. The model notes
explain the deliberate simplifications and the boundary between player and
developer material. Public tests contain no recovery walkthrough.

Inspired by PistonMiner's [BEESAT-1 recovery presentation at 38C3](https://media.ccc.de/v/38c3-hacking-yourself-a-satellite-recovering-beesat-1).
All spacecraft, protocols and radio links here are fictional.
