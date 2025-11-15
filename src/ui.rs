use crate::{
    memory::MemoryStats,
    powermetrics::{CoreMetrics, CpuMetrics, GpuMetrics},
    soc::SocInfo,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    prelude::*,
    text::Line,
    widgets::Sparkline,
    widgets::{Block, Borders, Gauge, Paragraph, Wrap},
};

pub struct UiSnapshot<'a> {
    pub soc: &'a SocInfo,
    pub cpu: &'a CpuMetrics,
    pub gpu: &'a GpuMetrics,
    pub memory: &'a MemoryStats,
    pub thermal_throttle: bool,
    pub color: Color,
    pub show_cores: bool,
    pub ane_percent: u64,
    pub ane_power_w: f64,
    pub ram_has_swap: bool,
    pub swap_used_gb: f64,
    pub swap_total_gb: f64,
    pub cpu_power: PowerSnapshot,
    pub gpu_power: PowerSnapshot,
    pub package_power: PowerSnapshot,
    pub cpu_history: Vec<f64>,
    pub gpu_history: Vec<f64>,
}

#[derive(Clone, Copy)]
pub struct PowerSnapshot {
    pub current: f64,
    pub average: f64,
    pub peak: f64,
    pub percent_of_tdp: f64,
}

pub fn draw(frame: &mut Frame<'_>, data: &UiSnapshot<'_>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(3),
            Constraint::Min(10),
        ])
        .split(frame.size());

    draw_processor(frame, chunks[0], data);
    draw_memory(frame, chunks[1], data);
    draw_power(frame, chunks[2], data);
}

fn draw_processor(frame: &mut Frame<'_>, area: Rect, data: &UiSnapshot<'_>) {
    let title = format!(
        "{} (cores: {}E+{}P+{}GPU)",
        data.soc.name, data.soc.e_core_count, data.soc.p_core_count, data.soc.gpu_core_count
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(data.color));
    frame.render_widget(block, area);

    let inner = area.inner(&Margin {
        horizontal: 1,
        vertical: 1,
    });
    let rows = if data.show_cores { 3 } else { 2 };
    let constraints = if data.show_cores {
        vec![
            Constraint::Length(5),
            Constraint::Length(5),
            Constraint::Min(0),
        ]
    } else {
        vec![Constraint::Length(5), Constraint::Length(5)]
    };
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let cpu_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(47),
            Constraint::Length(4),
            Constraint::Percentage(47),
        ])
        .split(sections[0]);

    let e_title = format!(
        "E-CPU Usage: {}% @ {} MHz",
        data.cpu.e_cluster_active, data.cpu.e_cluster_freq_mhz
    );
    let p_title = format!(
        "P-CPU Usage: {}% @ {} MHz",
        data.cpu.p_cluster_active, data.cpu.p_cluster_freq_mhz
    );
    render_usage_block(
        frame,
        cpu_chunks[0],
        e_title,
        data.cpu.e_cluster_active,
        data.color,
    );
    render_usage_block(
        frame,
        cpu_chunks[2],
        p_title,
        data.cpu.p_cluster_active,
        data.color,
    );

    let gpu_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(47),
            Constraint::Length(4),
            Constraint::Percentage(47),
        ])
        .split(sections[1]);

    let gpu_title = format!(
        "GPU Usage: {}% @ {} MHz",
        data.gpu.active_pct, data.gpu.freq_mhz
    );
    render_usage_block(
        frame,
        gpu_chunks[0],
        gpu_title,
        data.gpu.active_pct,
        data.color,
    );

    let ane_title = format!(
        "ANE Usage: {}% @ {:.1} W",
        data.ane_percent, data.ane_power_w
    );
    render_usage_block(
        frame,
        gpu_chunks[2],
        ane_title,
        data.ane_percent,
        data.color,
    );

    if data.show_cores && rows == 3 {
        let e_core_text = format_core_rows("E", &data.cpu.e_cores);
        let p_core_text = format_core_rows("P", &data.cpu.p_cores);
        let paragraph = Paragraph::new(format!("{e_core_text}\n{p_core_text}"))
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: true });
        frame.render_widget(paragraph, sections[2]);
    }
}

fn draw_memory(frame: &mut Frame<'_>, area: Rect, data: &UiSnapshot<'_>) {
    let ram_title = if data.ram_has_swap {
        format!(
            "RAM Usage: {:.1}/{:.1} GB - swap {:.1}/{:.1} GB",
            data.memory.used_gb, data.memory.total_gb, data.swap_used_gb, data.swap_total_gb
        )
    } else {
        format!(
            "RAM Usage: {:.1}/{:.1} GB - swap inactive",
            data.memory.used_gb, data.memory.total_gb
        )
    };
    let block = Block::default()
        .title("Memory")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(data.color));
    frame.render_widget(block, area);
    let inner = area.inner(&Margin {
        horizontal: 1,
        vertical: 1,
    });
    let gauge = Gauge::default()
        .block(Block::default().title(ram_title))
        .gauge_style(Style::default().fg(data.color))
        .percent(data.memory.free_percent as u16);
    frame.render_widget(gauge, inner);
}

fn draw_power(frame: &mut Frame<'_>, area: Rect, data: &UiSnapshot<'_>) {
    let block = Block::default()
        .title(format!(
            "CPU+GPU+ANE Power: {:.2}W (avg {:.2}W peak {:.2}W) throttle: {}",
            data.package_power.current,
            data.package_power.average,
            data.package_power.peak,
            if data.thermal_throttle { "yes" } else { "no" }
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(data.color));
    frame.render_widget(block, area);
    let inner = area.inner(&Margin {
        horizontal: 1,
        vertical: 1,
    });
    let charts = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    render_power_chart(
        frame,
        charts[0],
        data.color,
        "CPU",
        data.cpu_power,
        &data.cpu_history,
    );
    render_power_chart(
        frame,
        charts[1],
        data.color,
        "GPU",
        data.gpu_power,
        &data.gpu_history,
    );
}

fn render_power_chart(
    frame: &mut Frame<'_>,
    area: Rect,
    color: Color,
    label: &str,
    snapshot: PowerSnapshot,
    history: &[f64],
) {
    let title = format!(
        "{label}: {:.2}W ({:.0}% TDP) avg {:.2}W peak {:.2}W",
        snapshot.current, snapshot.percent_of_tdp, snapshot.average, snapshot.peak
    );
    let mut values: Vec<u64> = history
        .iter()
        .map(|v| (*v * 10.0).round().max(0.0) as u64)
        .collect();
    if values.is_empty() {
        values.push(0);
    }
    let max_value = values.iter().copied().max().unwrap_or(1).max(1);
    let spark = Sparkline::default()
        .block(Block::default().title(title))
        .style(Style::default().fg(color))
        .max(max_value)
        .data(&values);
    frame.render_widget(spark, area);
}

fn format_core_rows(prefix: &str, cores: &[CoreMetrics]) -> String {
    if cores.is_empty() {
        return format!("No {prefix}-cores detected");
    }
    let mut rows = Vec::new();
    for chunk in cores.chunks(4) {
        let row = chunk
            .iter()
            .map(|core| {
                format!(
                    "{prefix}{:02}: {:3}% @ {:>4}MHz",
                    core.id + 1,
                    core.active_pct,
                    core.freq_mhz
                )
            })
            .collect::<Vec<_>>()
            .join("  ");
        rows.push(row);
    }
    rows.join("\n")
}

fn render_usage_block(
    frame: &mut Frame<'_>,
    area: Rect,
    title: String,
    percent: u64,
    color: Color,
) {
    let bar_width = area.width.saturating_sub(2);
    let bar = block_bar(percent, bar_width);
    let lines = vec![Line::from(title), Line::from(bar)];
    let paragraph = Paragraph::new(lines)
        .style(Style::default().fg(color))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn block_bar(percent: u64, width: u16) -> String {
    let width = width.max(10) as usize;
    let clamped = percent.min(100) as usize;
    let filled = (clamped * width + 99) / 100;
    let empty = width.saturating_sub(filled);
    let filled_block = "█".repeat(filled);
    let empty_block = "░".repeat(empty);
    format!("{filled_block}{empty_block}")
}
