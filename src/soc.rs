use std::process::Command;

#[derive(Debug, Clone)]
pub struct SocInfo {
    pub name: String,
    pub e_core_count: u32,
    pub p_core_count: u32,
    pub gpu_core_count: u32,
    pub cpu_max_power: f64,
    pub gpu_max_power: f64,
}

impl SocInfo {
    pub fn detect() -> Self {
        let cpu_name =
            read_sysctl("machdep.cpu.brand_string").unwrap_or_else(|| "Apple Silicon".into());
        let e_core_count = read_sysctl("hw.perflevel1.logicalcpu")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let p_core_count = read_sysctl("hw.perflevel0.logicalcpu")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let gpu_core_count = read_gpu_core_count().unwrap_or(0);
        let (cpu_max_power, gpu_max_power) = lookup_caps(cpu_name.trim());

        Self {
            name: cpu_name.trim().to_string(),
            e_core_count,
            p_core_count,
            gpu_core_count,
            cpu_max_power,
            gpu_max_power,
        }
    }
}

fn lookup_caps(name: &str) -> (f64, f64) {
    if name.ends_with("Pro") {
        (40.0, 40.0)
    } else if name.ends_with("Max") {
        (90.0, 90.0)
    } else if name.ends_with("Ultra") {
        (140.0, 140.0)
    } else {
        (20.0, 20.0)
    }
}

fn read_sysctl(key: &str) -> Option<String> {
    let output = Command::new("/usr/sbin/sysctl")
        .args(["-n", key])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

fn read_gpu_core_count() -> Option<u32> {
    let output = Command::new("/usr/sbin/system_profiler")
        .args(["-detailLevel", "basic", "SPDisplaysDataType"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if let Some(rest) = line.trim().strip_prefix("Total Number of Cores: ") {
            if let Ok(value) = rest.trim().parse() {
                return Some(value);
            }
        }
    }
    None
}
