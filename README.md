# asitop In Rust

这是原 [asitop](https://github.com/tlkh/asitop) 的 Rust 重构版本。采用 Ratatui UI 构建UI，同时 Rust 版本修复了原 Python 版本存在的内存泄漏问题，并将长时间运行的内存占用降至4MB左右，约**原asitop短时运行的 10%**。详见原仓库 Issue [#80](https://github.com/tlkh/asitop/issues/80)。我曾在一个周末让原版 `asitop` 连续运行两天，内存泄漏最终导致 swap 写满了我的 512 GB 硬盘。
## 预览图

![默认视图](./IMG/IMG1.png)

开启 `--show-cores` 参数后的单核视图：

![单核视图](./IMG/IMG2.png)
## 功能特性

- 展示 CPU（集群 + 单核）、GPU、ANE 的块状占用条与功耗信息，支持滚动平均与峰值跟踪。
- 提供内存、交换分区、当前功耗、平均功耗、峰值功耗、网络与磁盘 I/O 速率等系统状态概览。
- 支持自定义刷新间隔、CPU&GPU功耗滚动平均窗口、配色方案，以及可选的单核视图与自动重启 `powermetrics`。

## 自行构建

```
cd asitop-rs
cargo build --release
```

可执行文件位于 `target/release/asitop`。

## 使用

`powermetrics` 需要 `sudo` 才能读取硬件计数器，运行示例：

```
sudo target/release/asitop --interval 1 --avg 30 --color 2
```

### 参数

- `--interval <seconds>`：刷新频率，同时也是 `powermetrics` 的采样间隔。
- `--avg <seconds>`：功耗读数的滚动平均窗口。
- `--color <0-8>`：选择预设配色。
        0 => Color::Reset,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        7 => Color::White,
        8 => Color::LightMagenta,
        _ => Color::Green,
- `--show-cores`：开启单核视图。
- `--max-count <n>`：采样达到 `n` 次后自动重启 `powermetrics`（0 表示永不重启）。
默认参数:
--interval 2 --avg 30 --color 1
按下 `q`、`Esc` 或 `Ctrl+C` 即可退出界面。
