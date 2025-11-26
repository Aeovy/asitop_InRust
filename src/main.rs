mod config;
mod io_stats;
mod memory;
mod powermetrics;
mod soc;
mod thermal;
mod ui;

use anyhow::{Context, Result};
use clap::Parser;
use config::Cli;
use crossterm::{
    cursor::Show,
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use io_stats::{IoSampler, IoStats};
use memory::{MemoryReader, MemoryStats};
use powermetrics::{
    CpuMetrics, GpuMetrics, History, PowermetricsReader, PowermetricsReading, RollingAverage,
    cleanup_powermetrics_files, new_timecode, run_powermetrics,
};
use ratatui::{Terminal, backend::CrosstermBackend, prelude::*};
use soc::SocInfo;
use std::{
    io::{self, stdout},
    process::Child,
    thread,
    time::{Duration, Instant},
};
use thermal::{ThermalLevel, read_warning_level};
use ui::{PowerSnapshot, UiSnapshot};

/// RAII wrapper for powermetrics child process.
/// Ensures the child process is killed and waited on when dropped,
/// preventing orphan processes on panic or early return.
struct PowermetricsGuard {
    child: Option<Child>,
}

impl PowermetricsGuard {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    /// Kill and restart the process with a new timecode
    fn restart(&mut self, timecode: &str, interval_ms: u64) -> Result<()> {
        // Kill existing process first
        if let Some(ref mut child) = self.child {
            child.kill().ok();
            child.wait().ok();
        }
        // Start new process
        self.child = Some(run_powermetrics(timecode, interval_ms)?);
        Ok(())
    }

    /// Explicitly stop the child process
    fn stop(&mut self) {
        if let Some(ref mut child) = self.child {
            child.kill().ok();
            child.wait().ok();
        }
        self.child = None;
    }
}

impl Drop for PowermetricsGuard {
    fn drop(&mut self) {
        // Ensure cleanup on panic or early return
        if let Some(ref mut child) = self.child {
            // Send SIGKILL and wait to reap the process
            child.kill().ok();
            child.wait().ok();
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    println!("\nASITOP_IN_RUST - An improved and refactored version of ASITOP, a performance monitoring CLI tool for Apple Silicon");
    println!("Original ASITOP https://github.com/tlkh/asitop");
    println!("Get help at https://github.com/Aeovy/asitop_InRust\n");
    println!("[1/3] Detecting SoC and preparing powermetrics\n");

    let soc = SocInfo::detect();
    let mut memory_reader = MemoryReader::new();
    let mut io_sampler = IoSampler::new();
    cleanup_powermetrics_files().ok();

    println!("[2/3] Starting powermetrics process\n");
    let mut timecode = new_timecode();
    let child =
        run_powermetrics(&timecode, cli.interval * 1000).context("failed to spawn powermetrics")?;
    // Wrap child in RAII guard to ensure cleanup on panic or early return
    let mut guard = PowermetricsGuard::new(child);
    let mut pm_reader = PowermetricsReader::new(&timecode);
    println!("[3/3] Waiting for first reading...\n");

    let first_reading = wait_for_reading(&mut pm_reader, Duration::from_millis(100))
        .context("powermetrics never produced a reading")?;

    let mut state = AppState::new(cli.clone(), soc, &mut memory_reader);
    state.apply_reading(first_reading, &mut io_sampler);
    state.memory_stats = memory_reader.read();

    let result = run_ui(
        &mut state,
        &mut guard,
        &mut timecode,
        &mut pm_reader,
        &mut memory_reader,
        &mut io_sampler,
    );

    // Explicitly stop before terminal cleanup for clean shutdown
    guard.stop();

    if let Err(err) = cleanup_terminal() {
        eprintln!("failed to restore terminal: {err}");
    }

    if let Err(err) = result {
        eprintln!("asitop exited with error: {err}");
        return Err(err);
    }

    Ok(())
}

/// Wait for powermetrics to produce the first reading, with timeout
fn wait_for_reading(reader: &mut PowermetricsReader, wait: Duration) -> Result<PowermetricsReading> {
    const MAX_ATTEMPTS: u32 = 300; // Wait up to 30 seconds (300 * 100ms)
    for attempt in 0..MAX_ATTEMPTS {
        if let Some(reading) = reader.parse()? {
            return Ok(reading);
        }
        if attempt % 50 == 49 {
            eprintln!("Still waiting for powermetrics data... ({} seconds)", (attempt + 1) / 10);
        }
        thread::sleep(wait);
    }
    anyhow::bail!("Timeout waiting for powermetrics data ({}s)", MAX_ATTEMPTS as u64 * wait.as_millis() as u64 / 1000)
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    Terminal::new(backend).map_err(Into::into)
}

fn cleanup_terminal() -> Result<()> {
    disable_raw_mode().ok();
    execute!(stdout(), Show, LeaveAlternateScreen).ok();
    Ok(())
}

fn run_ui(
    state: &mut AppState,
    guard: &mut PowermetricsGuard,
    timecode: &mut String,
    pm_reader: &mut PowermetricsReader,
    memory_reader: &mut MemoryReader,
    io_sampler: &mut IoSampler,
) -> Result<()> {
    let mut terminal = setup_terminal()?;
    let mut last_sample = Instant::now();
    let poll_rate = Duration::from_millis(100);
    let mut running = true;
    let mut needs_redraw = true;

    while running {
        if event::poll(poll_rate)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => running = false,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        running = false;
                    }
                    _ => {}
                }
            }
        }

        if last_sample.elapsed() >= Duration::from_millis(100) {
            if let Some(reading) = pm_reader.parse()? {
                if state.update_if_new(reading, memory_reader, io_sampler) {
                    last_sample = Instant::now();
                    needs_redraw = true;
                }
            }
        }

        if state.config.max_count > 0 && state.samples_taken >= state.config.max_count {
            *timecode = new_timecode();
            guard.restart(timecode, state.config.interval * 1000)?;
            pm_reader.set_timecode(timecode);
            state.samples_taken = 0;
            state.last_timestamp = None;
        }

        if needs_redraw {
            terminal.draw(|f| {
                let snapshot = state.snapshot();
                ui::draw(f, &snapshot);
            })?;
            needs_redraw = false;
        }
    }

    terminal.show_cursor().ok();
    Ok(())
}

fn color_from_arg(arg: u8) -> Color {
    match arg {
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
    }
}

struct AppState {
    config: Cli,
    soc: SocInfo,
    color: Color,
    memory_stats: MemoryStats,
    cpu_metrics: CpuMetrics,
    gpu_metrics: GpuMetrics,
    io_stats: IoStats,
    thermal_pressure: String,
    thermal_level: Option<ThermalLevel>,
    last_timestamp: Option<std::time::SystemTime>,
    power_history: History,
    cpu_avg: RollingAverage,
    gpu_avg: RollingAverage,
    package_avg: RollingAverage,
    cpu_peak: f32,
    gpu_peak: f32,
    package_peak: f32,
    cpu_power: f32,
    gpu_power: f32,
    package_power: f32,
    ane_percent: u64,
    ane_power: f32,
    pub samples_taken: u64,
}

impl AppState {
    fn new(cli: Cli, soc: SocInfo, memory_reader: &mut MemoryReader) -> Self {
        let interval_seconds = std::cmp::max(cli.interval, 1);
        let avg_window = std::cmp::max(1, (cli.avg / interval_seconds) as usize);
        let mut memory_stats = memory_reader.read();
        if (memory_stats.total_gb - memory_stats.used_gb).abs() < f32::EPSILON {
            memory_stats.used_gb = memory_stats.total_gb;
        }
        Self {
            color: color_from_arg(cli.color),
            config: cli,
            soc,
            memory_stats,
            cpu_metrics: CpuMetrics::default(),
            gpu_metrics: GpuMetrics::default(),
            io_stats: IoStats::default(),
            thermal_pressure: String::new(),
            thermal_level: None,
            last_timestamp: None,
            power_history: History::new(120),
            cpu_avg: RollingAverage::new(avg_window),
            gpu_avg: RollingAverage::new(avg_window),
            package_avg: RollingAverage::new(avg_window),
            cpu_peak: 0.0,
            gpu_peak: 0.0,
            package_peak: 0.0,
            cpu_power: 0.0,
            gpu_power: 0.0,
            package_power: 0.0,
            ane_percent: 0,
            ane_power: 0.0,
            samples_taken: 0,
        }
    }

    fn apply_reading(&mut self, reading: PowermetricsReading, io_sampler: &mut IoSampler) {
        self.last_timestamp = Some(reading.timestamp);
        self.thermal_pressure = reading.thermal_pressure;
        self.cpu_metrics = reading.cpu;
        self.gpu_metrics = reading.gpu;
        self.refresh_thermal_level();
        self.update_power_stats();
        self.refresh_io(io_sampler);
        self.samples_taken += 1;
    }

    fn update_if_new(
        &mut self,
        reading: PowermetricsReading,
        memory_reader: &mut MemoryReader,
        io_sampler: &mut IoSampler,
    ) -> bool {
        if let Some(last) = self.last_timestamp {
            if reading.timestamp <= last {
                return false;
            }
        }
        self.last_timestamp = Some(reading.timestamp);
        self.thermal_pressure = reading.thermal_pressure;
        self.cpu_metrics = reading.cpu;
        self.gpu_metrics = reading.gpu;
        self.memory_stats = memory_reader.read();
        self.refresh_thermal_level();
        self.update_power_stats();
        self.refresh_io(io_sampler);
        self.samples_taken += 1;
        true
    }

    fn refresh_io(&mut self, sampler: &mut IoSampler) {
        self.io_stats = sampler.sample();
    }

    fn update_power_stats(&mut self) {
        let interval = std::cmp::max(self.config.interval, 1) as f32;
        self.cpu_power = self.cpu_metrics.cpu_w / interval;
        self.gpu_power = self.cpu_metrics.gpu_w / interval;
        self.package_power = self.cpu_metrics.package_w / interval;
        self.ane_power = self.cpu_metrics.ane_w / interval;
        let ane_max = self.soc.ane_max_power.max(1.0);
        self.ane_percent = ((self.ane_power / ane_max) * 100.0).clamp(0.0, 100.0).round() as u64;

        self.cpu_peak = self.cpu_peak.max(self.cpu_power);
        self.gpu_peak = self.gpu_peak.max(self.gpu_power);
        self.package_peak = self.package_peak.max(self.package_power);
        self.cpu_avg.push(self.cpu_power);
        self.gpu_avg.push(self.gpu_power);
        self.package_avg.push(self.package_power);
        self.power_history.push(self.cpu_power + self.gpu_power);
    }

    fn snapshot(&self) -> UiSnapshot<'_> {
        let thermal_throttle = self
            .thermal_level
            .map(|level| level.is_throttled())
            .unwrap_or_else(|| self.thermal_pressure.trim() != "Nominal");
        UiSnapshot {
            soc: &self.soc,
            cpu: &self.cpu_metrics,
            gpu: &self.gpu_metrics,
            memory: &self.memory_stats,
            io: self.io_stats,
            thermal_throttle,
            color: self.color,
            show_cores: self.config.show_cores,
            ane_percent: self.ane_percent,
            ane_power_w: self.ane_power,
            ram_has_swap: self.memory_stats.swap_total_gb >= 0.1,
            swap_used_gb: self.memory_stats.swap_used_gb,
            swap_total_gb: self.memory_stats.swap_total_gb,
            cpu_power: PowerSnapshot {
                current: self.cpu_power,
                average: self.cpu_avg.average(),
                peak: self.cpu_peak,
                percent_of_tdp: if self.soc.cpu_max_power > 0.0 {
                    (self.cpu_power / self.soc.cpu_max_power * 100.0).clamp(0.0, 999.0)
                } else {
                    0.0
                },
            },
            gpu_power: PowerSnapshot {
                current: self.gpu_power,
                average: self.gpu_avg.average(),
                peak: self.gpu_peak,
                percent_of_tdp: if self.soc.gpu_max_power > 0.0 {
                    (self.gpu_power / self.soc.gpu_max_power * 100.0).clamp(0.0, 999.0)
                } else {
                    0.0
                },
            },
            package_power: PowerSnapshot {
                current: self.package_power,
                average: self.package_avg.average(),
                peak: self.package_peak,
                percent_of_tdp: 0.0,
            },
            power_history: self.power_history.values(),
        }
    }

    fn refresh_thermal_level(&mut self) {
        self.thermal_level = read_warning_level();
    }
}
