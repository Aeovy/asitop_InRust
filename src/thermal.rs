use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalLevel {
    Normal,
    Danger,
    Crisis,
    Unknown(u32),
}

impl ThermalLevel {
    pub fn is_throttled(self) -> bool {
        !matches!(self, ThermalLevel::Normal)
    }

    pub fn label(self) -> &'static str {
        match self {
            ThermalLevel::Normal => "Nominal",
            ThermalLevel::Danger => "Danger",
            ThermalLevel::Crisis => "Crisis",
            ThermalLevel::Unknown(_) => "Unknown",
        }
    }
}

impl fmt::Display for ThermalLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThermalLevel::Unknown(raw) => write!(f, "Unknown({raw})"),
            other => f.write_str(other.label()),
        }
    }
}

impl From<u32> for ThermalLevel {
    fn from(value: u32) -> Self {
        match value {
            0 => ThermalLevel::Normal,
            5 | 100 => ThermalLevel::Danger,
            10 | 110 => ThermalLevel::Crisis,
            other => ThermalLevel::Unknown(other),
        }
    }
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOPMGetThermalWarningLevel(level: *mut u32) -> i32;
}

pub fn read_warning_level() -> Option<ThermalLevel> {
    unsafe {
        let mut level = 0u32;
        let status = IOPMGetThermalWarningLevel(&mut level as *mut u32);
        if status == 0 {
            Some(ThermalLevel::from(level))
        } else {
            None
        }
    }
}
