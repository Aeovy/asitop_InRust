# asitop（Rust 版）

这是原始 [`asitop`](https://github.com/tlkh/asitop) 的 Rust 重写版本，完整复刻 Apple Silicon 上的监控体验。得益于零分配的 Ratatui UI 与安全的子进程管理，Rust 版不仅修复了 Python 版本的内存泄漏，还将常驻内存占用降低到 Python 版本的 1/10，长时间运行也保持稳定。

## 功能特性

- 通过 `sudo nice -n 10` 调用 `powermetrics`，直接解析 `/tmp` 中的 plist 数据流。
- 展示 CPU（集群 + 单核）、GPU、ANE 的块状占用条与功耗信息，支持滚动平均与峰值跟踪。
- 提供内存、交换分区、功耗历史、网络与磁盘 I/O 速率等系统状态概览。
- 支持自定义刷新间隔、滚动平均窗口、配色方案，以及可选的单核视图与自动重启 `powermetrics`。

## 构建

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

参数

- `--interval <seconds>`：刷新频率，同时也是 `powermetrics` 的采样间隔。
- `--avg <seconds>`：功耗读数的滚动平均窗口。
- `--color <0-8>`：选择预设配色。
- `--show-cores`：开启单核视图。
- `--max-count <n>`：采样达到 `n` 次后自动重启 `powermetrics`（0 表示永不重启）。

按下 `q`、`Esc` 或 `Ctrl+C` 即可退出界面，子进程会被自动清理，彻底避免 Python 版本的内存泄漏问题。
