use anyhow::{Context, Result};
use plist::{self, Date};
use serde::Deserialize;
use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{Cursor, Read, Seek, SeekFrom},
    process::{Child, Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

const POWER_FILE_PREFIX: &str = "/tmp/asitop_powermetrics";
const MAX_READ_BYTES: u64 = 1 * 1024 * 1024; // 1 MiB from EOF is enough for one sample

#[derive(Debug, Clone)]
pub struct PowermetricsReading {
    pub timestamp: SystemTime,
    pub thermal_pressure: String,
    pub cpu: CpuMetrics,
    pub gpu: GpuMetrics,
}

#[derive(Debug, Clone, Default)]
pub struct CpuMetrics {
    pub e_cluster_active: u64,
    pub e_cluster_freq_mhz: u64,
    pub p_cluster_active: u64,
    pub p_cluster_freq_mhz: u64,
    pub e_cores: Vec<CoreMetrics>,
    pub p_cores: Vec<CoreMetrics>,
    pub cpu_w: f64,
    pub gpu_w: f64,
    pub ane_w: f64,
    pub package_w: f64,
}

#[derive(Debug, Clone, Default)]
pub struct CoreMetrics {
    pub id: u32,
    pub active_pct: u64,
    pub freq_mhz: u64,
}

#[derive(Debug, Clone, Default)]
pub struct GpuMetrics {
    pub active_pct: u64,
    pub freq_mhz: u64,
}

#[derive(Debug, Deserialize)]
struct RawSnapshot {
    timestamp: Date,
    thermal_pressure: String,
    processor: RawProcessor,
    gpu: RawGpu,
}

#[derive(Debug, Deserialize)]
struct RawProcessor {
    clusters: Vec<RawCluster>,
    #[serde(default)]
    ane_energy: f64,
    #[serde(default)]
    cpu_energy: f64,
    #[serde(default)]
    gpu_energy: f64,
    #[serde(default)]
    combined_power: f64,
}

#[derive(Debug, Deserialize)]
struct RawCluster {
    name: String,
    freq_hz: f64,
    idle_ratio: f64,
    #[serde(default)]
    cpus: Vec<RawCore>,
}

#[derive(Debug, Clone)]
struct ClusterData {
    name: String,
    active_pct: u64,
    freq_mhz: u64,
}

#[derive(Debug, Deserialize)]
struct RawCore {
    cpu: u32,
    freq_hz: f64,
    idle_ratio: f64,
}

#[derive(Debug, Deserialize)]
struct RawGpu {
    freq_hz: f64,
    idle_ratio: f64,
}

pub fn powermetrics_path(timecode: &str) -> String {
    format!("{POWER_FILE_PREFIX}{timecode}")
}

pub fn run_powermetrics(timecode: &str, interval_ms: u64) -> Result<Child> {
    cleanup_powermetrics_files().ok();
    let path = powermetrics_path(timecode);
    let interval_arg = interval_ms.to_string();
    let mut cmd = Command::new("sudo");
    cmd.args([
        "nice",
        "-n",
        "10",
        "powermetrics",
        "--samplers",
        "cpu_power,gpu_power,thermal",
        "-o",
        &path,
        "-f",
        "plist",
        "-i",
        &interval_arg,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());

    cmd.spawn().with_context(|| "failed to spawn powermetrics")
}

pub fn cleanup_powermetrics_files() -> Result<()> {
    if let Ok(entries) = fs::read_dir("/tmp") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("asitop_powermetrics") {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }
    Ok(())
}

pub fn new_timecode() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    now.to_string()
}

pub fn parse_powermetrics(timecode: &str) -> Result<Option<PowermetricsReading>> {
    let path = powermetrics_path(timecode);
    let mut file = match File::open(&path) {
        Ok(f) => f,
        Err(_) => return Ok(None),
    };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(MAX_READ_BYTES);
    file.seek(SeekFrom::Start(start)).ok();
    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .with_context(|| "failed to read powermetrics chunk")?;
    let chunks: Vec<&[u8]> = data
        .split(|b| *b == 0)
        .filter(|chunk| !chunk.is_empty())
        .collect();
    for chunk in chunks.iter().rev() {
        if let Ok(snapshot) = plist::from_reader::<_, RawSnapshot>(Cursor::new(chunk)) {
            return Ok(Some(convert_snapshot(snapshot)));
        }
    }
    Ok(None)
}

fn convert_snapshot(raw: RawSnapshot) -> PowermetricsReading {
    let timestamp = raw.timestamp.into();
    let mut e_clusters: Vec<ClusterData> = Vec::new();
    let mut p_clusters: Vec<ClusterData> = Vec::new();
    let mut e_cores = Vec::new();
    let mut p_cores = Vec::new();

    for cluster in raw.processor.clusters {
        let RawCluster {
            name,
            freq_hz,
            idle_ratio,
            cpus,
        } = cluster;
        let freq_mhz = display_freq(freq_hz);
        let active = ratio_to_pct(idle_ratio);
        let is_e = name.starts_with(['E', 'e']);
        if is_e {
            e_clusters.push(ClusterData {
                name: name.clone(),
                active_pct: active,
                freq_mhz,
            });
        } else if name.starts_with(['P', 'p']) {
            p_clusters.push(ClusterData {
                name: name.clone(),
                active_pct: active,
                freq_mhz,
            });
        }
        for core in cpus {
            let metrics = CoreMetrics {
                id: core.cpu,
                active_pct: ratio_to_pct(core.idle_ratio),
                freq_mhz: display_freq(core.freq_hz),
            };
            if is_e {
                e_cores.push(metrics);
            } else {
                p_cores.push(metrics);
            }
        }
    }

    let (e_cluster_active, e_cluster_freq) = aggregate_cluster(&e_clusters, &e_cores, 'E');
    let (p_cluster_active, p_cluster_freq) = aggregate_cluster(&p_clusters, &p_cores, 'P');

    PowermetricsReading {
        timestamp,
        thermal_pressure: raw.thermal_pressure,
        cpu: CpuMetrics {
            e_cluster_active,
            e_cluster_freq_mhz: e_cluster_freq,
            p_cluster_active,
            p_cluster_freq_mhz: p_cluster_freq,
            e_cores,
            p_cores,
            cpu_w: raw.processor.cpu_energy / 1000.0,
            gpu_w: raw.processor.gpu_energy / 1000.0,
            ane_w: raw.processor.ane_energy / 1000.0,
            package_w: raw.processor.combined_power / 1000.0,
        },
        gpu: GpuMetrics {
            active_pct: ratio_to_pct(raw.gpu.idle_ratio),
            freq_mhz: display_freq(raw.gpu.freq_hz),
        },
    }
}

fn display_freq(freq_hz: f64) -> u64 {
    if !freq_hz.is_finite() || freq_hz <= 0.0 {
        0
    } else if freq_hz >= 100_000.0 {
        (freq_hz / 1_000_000.0).round() as u64
    } else {
        freq_hz.round() as u64
    }
}

fn ratio_to_pct(idle_ratio: f64) -> u64 {
    if !idle_ratio.is_finite() {
        return 0;
    }
    let ratio = if idle_ratio > 1.0 {
        idle_ratio / 100.0
    } else {
        idle_ratio
    };
    let ratio = ratio.clamp(0.0, 1.0);
    ((1.0 - ratio) * 100.0).round() as u64
}

fn aggregate_cluster(clusters: &[ClusterData], cores: &[CoreMetrics], prefix: char) -> (u64, u64) {
    let primary_label = format!("{prefix}-Cluster");
    if let Some(primary) = clusters.iter().find(|c| c.name == primary_label) {
        let active = if primary.active_pct > 0 {
            primary.active_pct
        } else {
            core_average(cores)
        };
        let freq = if primary.freq_mhz > 0 {
            primary.freq_mhz
        } else {
            core_max_freq(cores)
        };
        return (active, freq);
    }

    let matching: Vec<&ClusterData> = clusters
        .iter()
        .filter(|c| c.name.starts_with(prefix))
        .collect();
    if !matching.is_empty() {
        let active_sum: u64 = matching.iter().map(|c| c.active_pct).sum();
        let freq_max = matching.iter().map(|c| c.freq_mhz).max().unwrap_or(0);
        let avg_active = if active_sum > 0 {
            active_sum / matching.len() as u64
        } else {
            core_average(cores)
        };
        let freq = if freq_max > 0 {
            freq_max
        } else {
            core_max_freq(cores)
        };
        return (avg_active, freq);
    }

    (core_average(cores), core_max_freq(cores))
}

fn core_average(cores: &[CoreMetrics]) -> u64 {
    if cores.is_empty() {
        0
    } else {
        let sum: u64 = cores.iter().map(|c| c.active_pct).sum();
        sum / cores.len() as u64
    }
}

fn core_max_freq(cores: &[CoreMetrics]) -> u64 {
    cores.iter().map(|c| c.freq_mhz).max().unwrap_or(0)
}

/// Helper storing datapoints for sparkline-style history charts.
#[derive(Default)]
pub struct History {
    data: VecDeque<f64>,
    max_len: usize,
}

impl History {
    pub fn new(max_len: usize) -> Self {
        Self {
            data: VecDeque::with_capacity(max_len),
            max_len,
        }
    }

    pub fn push(&mut self, value: f64) {
        if self.data.len() == self.max_len {
            self.data.pop_front();
        }
        self.data.push_back(value);
    }

    pub fn values(&self) -> Vec<f64> {
        self.data.iter().copied().collect()
    }
}

#[derive(Default)]
pub struct RollingAverage {
    data: VecDeque<f64>,
    max_len: usize,
    sum: f64,
}

impl RollingAverage {
    pub fn new(max_len: usize) -> Self {
        Self {
            data: VecDeque::with_capacity(max_len),
            max_len,
            sum: 0.0,
        }
    }

    pub fn push(&mut self, value: f64) {
        if self.max_len == 0 {
            return;
        }
        if self.data.len() == self.max_len {
            if let Some(front) = self.data.pop_front() {
                self.sum -= front;
            }
        }
        self.sum += value;
        self.data.push_back(value);
    }

    pub fn average(&self) -> f64 {
        if self.data.is_empty() {
            0.0
        } else {
            self.sum / self.data.len() as f64
        }
    }
}
