use clap::Parser;

/// Command line options controlling sampling and layout.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "asitop",
    version,
    about = "Apple Silicon performance monitor rewritten in Rust"
)]
pub struct Cli {
    /// Display interval in seconds. This is also passed to powermetrics.
    #[arg(long, default_value_t = 1, value_name = "SECONDS")]
    pub interval: u64,

    /// UI color (0-8) to match the classic asitop palette.
    #[arg(long, default_value_t = 2)]
    pub color: u8,

    /// Interval (in seconds) used for computing rolling averages.
    #[arg(long, default_value_t = 30, value_name = "SECONDS")]
    pub avg: u64,

    /// When true, render per-core information instead of compact gauges.
    #[arg(long, default_value_t = false)]
    pub show_cores: bool,

    /// Restart powermetrics after this many samples (0 = never restart).
    #[arg(long, default_value_t = 0, value_name = "COUNT")]
    pub max_count: u64,
}
