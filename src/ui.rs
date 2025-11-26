use crate::{
    io_stats::IoStats,
    memory::MemoryStats,
    powermetrics::{CoreMetrics, CpuMetrics, GpuMetrics},
    soc::SocInfo,
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    prelude::*,
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, RenderDirection, Sparkline, Wrap},
};

const CORE_MAX_COLUMNS: usize = 4;
const CORE_FIXED_WIDTH: usize = 18;
const CORE_MIN_BAR_WIDTH: usize = 6;
const CORE_MIN_ENTRY_WIDTH: usize = CORE_FIXED_WIDTH + CORE_MIN_BAR_WIDTH;

pub struct UiSnapshot<'a> {
    pub soc: &'a SocInfo,
    pub cpu: &'a CpuMetrics,
    pub gpu: &'a GpuMetrics,
    pub memory: &'a MemoryStats,
    pub io: IoStats,
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
    pub power_history: Vec<f64>,
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
            Constraint::Length(5),
            Constraint::Min(10),
        ])
        .split(frame.area());

    draw_processor(frame, chunks[0], data);
    draw_memory(frame, chunks[1], data);
    draw_io(frame, chunks[2], data);
    draw_power(frame, chunks[3], data);
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

    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let mut constraints = vec![Constraint::Length(2), Constraint::Length(2)];
    if data.show_cores {
        constraints.push(Constraint::Min(0));
    }
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    let cpu_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(49),
            Constraint::Length(2),
            Constraint::Percentage(49),
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
            Constraint::Percentage(49),
            Constraint::Length(2),
            Constraint::Percentage(49),
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

    if data.show_cores {
        render_core_sections(frame, sections[2], data);
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
    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let gauge = Gauge::default()
        .block(Block::default().title(ram_title))
        .gauge_style(Style::default().fg(data.color))
        .percent(data.memory.used_percent as u16);
    frame.render_widget(gauge, inner);
}

fn draw_io(frame: &mut Frame<'_>, area: Rect, data: &UiSnapshot<'_>) {
    let block = Block::default()
        .title("I/O")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(data.color));
    frame.render_widget(block, area);
    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);
    render_io_panel(
        frame,
        columns[0],
        "Network I/O",
        "In",
        format_rate(data.io.net_in_mbps),
        "Out",
        format_rate(data.io.net_out_mbps),
        data.color,
    );
    render_io_panel(
        frame,
        columns[1],
        "Disk I/O",
        "Read",
        format_rate(data.io.disk_read_mbps),
        "Write",
        format_rate(data.io.disk_write_mbps),
        data.color,
    );
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
    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    let segments = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(inner);

    render_power_summary(frame, segments[0], data);
    render_power_history(frame, segments[1], data);
}
fn render_power_summary(frame: &mut Frame<'_>, area: Rect, data: &UiSnapshot<'_>) {
    let cpu_line = format!(
        "CPU: {:.2}W ({:.0}% TDP) avg {:.2}W peak {:.2}W",
        data.cpu_power.current,
        data.cpu_power.percent_of_tdp,
        data.cpu_power.average,
        data.cpu_power.peak
    );
    let gpu_line = format!(
        "GPU: {:.2}W ({:.0}% TDP) avg {:.2}W peak {:.2}W",
        data.gpu_power.current,
        data.gpu_power.percent_of_tdp,
        data.gpu_power.average,
        data.gpu_power.peak
    );
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);
    let cpu_paragraph = Paragraph::new(Line::from(cpu_line))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });
    let gpu_paragraph = Paragraph::new(Line::from(gpu_line))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });
    frame.render_widget(cpu_paragraph, columns[0]);
    frame.render_widget(gpu_paragraph, columns[1]);
}

fn render_power_history(frame: &mut Frame<'_>, area: Rect, data: &UiSnapshot<'_>) {
    let peak_limit = data.package_power.peak.max(0.1);
    let mut values = combined_history_values(&data.power_history, peak_limit);
    if area.width > 0 {
        let max_points = area.width as usize;
        if values.len() > max_points {
            let start = values.len() - max_points;
            values = values[start..].to_vec();
        } else if values.len() < max_points {
            let mut padded = vec![0; max_points - values.len()];
            padded.extend(values);
            values = padded;
        }
    }
    if values.is_empty() {
        values.push(0);
    }
    let max_value = values.iter().copied().max().unwrap_or(100).max(100);
    let spark = Sparkline::default()
        .style(Style::default().fg(data.color))
        .direction(RenderDirection::LeftToRight)
        .max(max_value)
        .data(&values);
    frame.render_widget(spark, area);
}

fn combined_history_values(history: &[f64], peak_limit: f64) -> Vec<u64> {
    history
        .iter()
        .map(|value| {
            if peak_limit <= 0.0 {
                (*value * 10.0).max(0.0).round() as u64
            } else {
                ((*value / peak_limit) * 100.0).round() as u64
            }
        })
        .collect()
}

fn render_core_sections(frame: &mut Frame<'_>, area: Rect, data: &UiSnapshot<'_>) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    render_core_panel(
        frame,
        columns[0],
        "E-Cores",
        "E",
        &data.cpu.e_cores,
        data.color,
    );
    render_core_panel(
        frame,
        columns[1],
        "P-Cores",
        "P",
        &data.cpu.p_cores,
        data.color,
    );
}

fn render_core_panel(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    prefix: &str,
    cores: &[CoreMetrics],
    accent: Color,
) {
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent));
    frame.render_widget(block, area);
    let inner = area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let columns = core_columns(inner.width, cores.len());
    let entry_width = if columns == 0 {
        inner.width as usize
    } else {
        (inner.width as usize).max(1) / columns
    };
    let available = entry_width.saturating_sub(CORE_FIXED_WIDTH);
    let bar_width = available.max(1);

    let mut lines: Vec<Line<'static>> = Vec::new();
    if cores.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "未检测到核心",
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        for chunk in cores.chunks(columns.max(1)) {
            let mut spans: Vec<Span<'static>> = Vec::new();
            for core in chunk {
                spans.extend(core_entry_spans(
                    prefix,
                    core,
                    bar_width,
                    accent,
                    entry_width,
                ));
            }
            lines.push(Line::from(spans));
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

fn core_columns(width: u16, count: usize) -> usize {
    if count == 0 {
        return 1;
    }
    let width = width as usize;
    let mut columns = width / CORE_MIN_ENTRY_WIDTH;
    if columns == 0 {
        columns = 1;
    }
    columns = columns.min(CORE_MAX_COLUMNS);
    columns.min(count)
}

fn core_entry_spans(
    prefix: &str,
    core: &CoreMetrics,
    bar_width: usize,
    accent: Color,
    entry_width: usize,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut consumed = 0;
    let label = format!("{prefix}{:02}", core.id + 1);
    let label_text = format!("{label} ");
    consumed += label_text.chars().count();
    spans.push(Span::styled(
        label_text,
        Style::default().fg(accent).add_modifier(Modifier::BOLD),
    ));

    let clamped = core.active_pct.min(100) as usize;
    let filled = ((clamped * bar_width) + 99) / 100;
    let empty = bar_width.saturating_sub(filled);
    if filled > 0 {
        let block = "█".repeat(filled);
        consumed += block.chars().count();
        spans.push(Span::styled(
            block,
            Style::default().fg(core_usage_color(core.active_pct)),
        ));
    }
    if empty > 0 {
        let pad = "░".repeat(empty);
        consumed += pad.chars().count();
        spans.push(Span::styled(pad, Style::default().fg(Color::DarkGray)));
    }

    spans.push(Span::raw(" "));
    consumed += 1;
    let percent_text = format!("{:>3}%", core.active_pct.min(999));
    consumed += percent_text.chars().count();
    spans.push(Span::styled(
        percent_text,
        Style::default()
            .fg(core_usage_color(core.active_pct))
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::raw(" "));
    consumed += 1;
    let freq_text = format!("{:>4}MHz", core.freq_mhz);
    consumed += freq_text.chars().count();
    spans.push(Span::styled(
        freq_text,
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    ));

    if consumed < entry_width {
        spans.push(Span::raw(" ".repeat(entry_width - consumed)));
    }

    spans
}

fn core_usage_color(percent: u64) -> Color {
    match percent {
        90..=u64::MAX => Color::Red,
        70..=89 => Color::LightRed,
        50..=69 => Color::Yellow,
        30..=49 => Color::LightGreen,
        _ => Color::Cyan,
    }
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

fn render_io_panel(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    first_label: &str,
    first_value: String,
    second_label: &str,
    second_value: String,
    color: Color,
) {
    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{first_label:<5}"),
                Style::default().fg(Color::Gray),
            ),
            Span::styled(first_value, Style::default().fg(color)),
        ]),
        Line::from(vec![
            Span::styled(
                format!("{second_label:<5}"),
                Style::default().fg(Color::Gray),
            ),
            Span::styled(second_value, Style::default().fg(color)),
        ]),
    ];
    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::NONE)
                .title_alignment(Alignment::Left),
        )
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn format_rate(mbps: f64) -> String {
    let value = mbps.max(0.0);
    if value >= 1024.0 {
        format!("{:.2} GB/s", value / 1024.0)
    } else if value >= 1.0 {
        format!("{:.2} MB/s", value)
    } else if value >= 0.01 {
        format!("{:.1} KB/s", value * 1024.0)
    } else {
        format!("{:.0} B/s", (value * 1024.0 * 1024.0).round())
    }
}
