# asitop In Rust

This is the Rust refactored version of the original [asitop](https://github.com/tlkh/asitop). Built with the Ratatui UI, the Rust version fixes the memory leak issue present in the original Python version (see Issue [#80](https://github.com/tlkh/asitop/issues/80) in the original repository) and reduces long-term memory usage to around 4MB, approximately **25% of the short-term memory usage of the original asitop**.

## Memory Usage in Rust Version: 3.8MB
![Memory Usage in Rust Version](../IMG/MEMRUST.png)

## Memory Usage in Python Version: 16.8MB
![Memory Usage in Python Version](../IMG/MEMPYTHON.png)

I once ran the original asitop for two days over a weekend, and the memory leak completely filled the swap space on my 512 GB hard drive.

## Preview

![Default View](../IMG/IMG1.png)

Single-core view enabled with the `--show-cores` parameter:

![Single-Core View](../IMG/IMG2.png)

## Features

- Displays block usage bars and power consumption information for CPU (clusters + single cores), GPU, and ANE, with support for rolling averages and peak tracking.
- Provides an overview of system status, including memory, swap, current power consumption, average power consumption, peak power consumption, network, and disk I/O rates.
- Supports customizable refresh intervals, rolling average windows for CPU & GPU power consumption, color schemes, as well as optional single-core views and automatic `powermetrics` restarts.
- UI layout adapts to terminal window size.

## Build Instructions

```bash
cd asitop_InRust
cargo build --release
```

The executable will be located at `target/release/asitop_in_rust`.
### Install

```bash
cargo install --path .
```

## Usage

`powermetrics` requires `sudo` to read hardware counters. Example usage:

```bash
sudo target/release/asitop_in_rust --interval 2 --avg 30 --color 2 --show-cores
```

### Parameters

- `--interval <seconds>`: Refresh rate, which is also the sampling interval for `powermetrics`.
- `--avg <seconds>`: Rolling average window for power readings.
- `--color <0-8>`: Select a preset color scheme.

  | Value | Color        |
  |-------|--------------|
  |  0    | Black        |
  |  1    | Red          |
  |  2    | Green        |
  |  3    | Yellow       |
  |  4    | Blue         |
  |  5    | Magenta      |
  |  6    | Cyan         |
  |  7    | White        |
  |  8    | LightMagenta |

  Default: `Green`
- `--show-cores`: Enable single-core view.
- `--max-count <n>`: Automatically restart `powermetrics` after `n` samples (0 means never restart).

Default parameters:
`--interval 2 --avg 30 --color 1`

Press `q`, `Esc`, or `Ctrl+C` to exit the interface.