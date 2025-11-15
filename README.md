# asitop (Rust)

This is a Rust rewrite of the original [`asitop`](https://github.com/tlkh/asitop) tool. It delivers the same Apple Silicon monitoring experience with a zero-allocation Ratatui UI and safe process management that avoids the memory leak present in the Python version.

## Features

- Launches `powermetrics` under `sudo nice -n 10` and parses the plist stream directly from `/tmp`.
- Displays CPU (cluster + per-core), GPU, and ANE utilization gauges.
- Shows memory usage, swap activity, and CPU/GPU power charts with rolling averages and peak tracking.
- Supports optional per-core view, custom refresh interval, UI color presets, and automatic powermetrics restarts.

## Building

```
cd asitop-rs
cargo build --release
```

The resulting binary will be at `target/release/asitop`.

## Usage

Run `asitop` with `sudo` so that `powermetrics` can access the required counters:

```
sudo target/release/asitop --interval 1 --avg 30 --color 2
```

Command line options (matching the legacy tool):

- `--interval <seconds>` – Update frequency and powermetrics sampling interval.
- `--avg <seconds>` – Rolling average window for the power readouts.
- `--color <0-8>` – Pick one of the built-in color presets.
- `--show-cores` – Toggle detailed per-core view.
- `--max-count <n>` – Automatically restart `powermetrics` after `n` samples (0 = never).

Press `q`, `Esc`, or `Ctrl+C` to exit the UI. The powermetrics child process is always cleaned up, preventing the leak that existed in the Python implementation.
