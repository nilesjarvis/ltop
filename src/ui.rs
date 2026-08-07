use crate::api::GpuInfo;
use crate::app::{App, PromptRateBasis, Section};
use crate::chart::BrailleChart;
use crate::theme::{Gradient, ThemeColors};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, Padding, Paragraph, Row, Table, Widget, Wrap,
};
use ratatui::Frame;

const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 20;
const DASHBOARD_WIDTH: u16 = 118;
const DASHBOARD_HEIGHT: u16 = 30;
const BRAND_TITLE: &str = " 🦙 ltop ";

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let colors = app.theme.colors();
    let background = if app.theme_background {
        colors.surface
    } else {
        Color::Reset
    };
    frame.render_widget(
        Block::default().style(Style::default().fg(colors.text).bg(background)),
        area,
    );

    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        draw_too_small(frame, area, colors);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(1),
            Constraint::Min(14),
            Constraint::Length(1),
        ])
        .split(area);

    draw_header(frame, chunks[0], app);
    draw_body(frame, chunks[2], app);
    draw_footer(frame, chunks[3], app);

    if app.show_help {
        draw_help(frame, area, app);
    }
    if app.show_theme_picker {
        draw_theme_picker(frame, area, app);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let colors = app.theme.colors();
    let (status, status_color) = connection_status(app);
    let title = Line::from(vec![
        Span::styled(
            BRAND_TITLE,
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("llama.cpp monitor ", Style::default().fg(colors.dim)),
    ]);
    let status_title = Line::from(Span::styled(
        format!("● {status} · {} ", app.update_interval_label()),
        Style::default()
            .fg(status_color)
            .add_modifier(Modifier::BOLD),
    ))
    .right_aligned();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors.border))
        .padding(Padding::horizontal(1))
        .title(title)
        .title(status_title);
    let inner = block.inner(area);
    block.render(area, frame.buffer_mut());

    let props = app.snapshot.props.as_ref();
    let model = props
        .map(|props| props.model_alias.as_str())
        .filter(|model| !model.is_empty())
        .unwrap_or("waiting for server");
    let ftype = props
        .map(|props| props.model_ftype.as_str())
        .filter(|ftype| !ftype.is_empty());
    let slot_count = props
        .map(|props| props.total_slots)
        .unwrap_or(app.snapshot.slots.len() as i64);
    let context = props.map(|props| props.n_ctx).unwrap_or(0);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(inner);

    let mut model_spans = vec![
        label_span("MODEL  ", colors),
        Span::styled(model.to_string(), value_style(colors)),
    ];
    if let Some(ftype) = ftype {
        model_spans.push(Span::styled(
            format!("  ·  {ftype}"),
            Style::default().fg(colors.dim),
        ));
    }

    if inner.width >= 98 {
        let top = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(64), Constraint::Fill(1)])
            .split(rows[0]);
        frame.render_widget(Paragraph::new(Line::from(model_spans)), top[0]);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label_span("TOKENS  ", colors),
                Span::styled(
                    format!("↑ eval {}", fmt_num(app.total_prompt_tokens)),
                    Style::default().fg(colors.prompt),
                ),
                Span::styled("   ", Style::default()),
                Span::styled(
                    format!("↓ out {}", fmt_num(app.total_predict_tokens)),
                    Style::default().fg(colors.predict),
                ),
            ])),
            top[1],
        );

        let bottom = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(34),
                Constraint::Percentage(40),
                Constraint::Fill(1),
            ])
            .split(rows[1]);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label_span("SERVER  ", colors),
                Span::styled(app.url.clone(), Style::default().fg(colors.accent)),
            ])),
            bottom[0],
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label_span("GPU  ", colors),
                Span::styled(hardware_summary(app), value_style(colors)),
            ])),
            bottom[1],
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label_span("RUN  ", colors),
                Span::styled(
                    format!(
                        "{} · {} ctx · {}",
                        fmt_slot_count(slot_count),
                        fmt_num(context as f64),
                        app.uptime_str()
                    ),
                    Style::default().fg(colors.text),
                ),
            ])),
            bottom[2],
        );
    } else {
        frame.render_widget(Paragraph::new(Line::from(model_spans)), rows[0]);
        let bottom = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(58), Constraint::Fill(1)])
            .split(rows[1]);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label_span("SERVER  ", colors),
                Span::styled(app.url.clone(), Style::default().fg(colors.accent)),
            ])),
            bottom[0],
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label_span("RUN  ", colors),
                Span::styled(
                    format!(
                        "{} · {} ctx · {}",
                        fmt_slot_count(slot_count),
                        fmt_num(context as f64),
                        app.uptime_str()
                    ),
                    Style::default().fg(colors.text),
                ),
            ])),
            bottom[1],
        );
    }
}

fn draw_body(frame: &mut Frame, area: Rect, app: &App) {
    if area.width >= DASHBOARD_WIDTH && area.height >= DASHBOARD_HEIGHT {
        draw_dashboard(frame, area, app);
    } else {
        draw_section(frame, area, app);
    }
}

fn draw_dashboard(frame: &mut Frame, area: Rect, app: &App) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(58),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(area);

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Length(1),
            Constraint::Percentage(34),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(columns[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(31),
            Constraint::Length(1),
            Constraint::Percentage(43),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(columns[2]);

    let throughput_focused = app.current_section == Section::Throughput;
    draw_throughput_chart(frame, left[0], app, true, throughput_focused);
    draw_throughput_chart(frame, left[2], app, false, throughput_focused);

    let system = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(left[4]);
    let gpu_focused = app.current_section == Section::Gpu;
    draw_gpu_chart(frame, system[0], app, gpu_focused);
    draw_power_chart(frame, system[2], app, gpu_focused);

    draw_slots_panel(frame, right[0], app, app.current_section == Section::Slots);
    draw_gpu_panel(frame, right[2], app, gpu_focused);
    draw_stats_panel(
        frame,
        right[4],
        app,
        app.current_section == Section::Overview,
    );
}

fn draw_section(frame: &mut Frame, area: Rect, app: &App) {
    match app.current_section {
        Section::Overview | Section::Help => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(45),
                    Constraint::Length(1),
                    Constraint::Fill(1),
                ])
                .split(area);
            draw_stats_panel(frame, rows[0], app, true);
            let system = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(50),
                    Constraint::Length(1),
                    Constraint::Fill(1),
                ])
                .split(rows[2]);
            draw_gpu_chart(frame, system[0], app, false);
            draw_power_chart(frame, system[2], app, false);
        }
        Section::Throughput => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(50),
                    Constraint::Length(1),
                    Constraint::Fill(1),
                ])
                .split(area);
            draw_throughput_chart(frame, rows[0], app, true, true);
            draw_throughput_chart(frame, rows[2], app, false, true);
        }
        Section::Slots => draw_slots_panel(frame, area, app, true),
        Section::Gpu => {
            if area.height >= 16 {
                let rows = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Percentage(60),
                        Constraint::Length(1),
                        Constraint::Fill(1),
                    ])
                    .split(area);
                draw_gpu_panel(frame, rows[0], app, true);
                let system = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(50),
                        Constraint::Length(1),
                        Constraint::Fill(1),
                    ])
                    .split(rows[2]);
                draw_gpu_chart(frame, system[0], app, true);
                draw_power_chart(frame, system[2], app, true);
            } else {
                draw_gpu_panel(frame, area, app, true);
            }
        }
    }
}

fn draw_throughput_chart(frame: &mut Frame, area: Rect, app: &App, prompt: bool, focused: bool) {
    let colors = app.theme.colors();
    let (name, history, current_value, gradient, color) = if prompt {
        (
            "PROMPT EVAL",
            &app.prompt_rate_history,
            match app.prompt_rate_basis {
                PromptRateBasis::Interval => format!("{:.1} tok/s last ", app.prompt_rate),
                PromptRateBasis::ServerAverage => format!("{:.1} tok/s avg ", app.prompt_rate),
                PromptRateBasis::Unavailable => "waiting ".to_string(),
            },
            &app.theme.prompt,
            colors.prompt,
        )
    } else {
        (
            "GENERATE",
            &app.predict_rate_history,
            format!("{:.1} tok/s ", app.predict_rate),
            &app.theme.predict,
            colors.predict,
        )
    };
    let data = app.history_as_points(history);
    let block = panel_block(
        panel_title(name, colors.title),
        panel_value(current_value, color),
        focused,
        app.theme.throughput_border,
        colors,
    );
    frame.render_widget(
        BrailleChart::new(
            &data,
            app.max_history(history),
            gradient,
            app.theme.graph_text,
        )
        .block(block),
        area,
    );
}

fn draw_gpu_chart(frame: &mut Frame, area: Rect, app: &App, focused: bool) {
    let colors = app.theme.colors();
    let data = app.history_as_points(&app.gpu_util_history);
    let current = app
        .gpu_util_history
        .back()
        .copied()
        .unwrap_or_else(|| average_gpu_util(app));
    let current_color = app.theme.gpu.at(current);
    let block = panel_block(
        panel_title("GPU UTIL", colors.title),
        panel_value(format!("{current:.0}% "), current_color),
        focused,
        app.theme.gpu_border,
        colors,
    );
    frame.render_widget(
        BrailleChart::new(&data, 100.0, &app.theme.gpu, app.theme.graph_text).block(block),
        area,
    );
}

fn draw_power_chart(frame: &mut Frame, area: Rect, app: &App, focused: bool) {
    let colors = app.theme.colors();
    let data = app.history_as_points(&app.power_history);
    let current = app
        .power_history
        .back()
        .copied()
        .unwrap_or_else(|| app.snapshot.gpus.iter().map(|gpu| gpu.power_draw).sum());
    let max = app.max_history(&app.power_history);
    let current_color = app.theme.power.at(current / max.max(1.0) * 100.0);
    let block = panel_block(
        panel_title("POWER", colors.title),
        panel_value(format!("{current:.0} W "), current_color),
        focused,
        app.theme.gpu_border,
        colors,
    );
    frame.render_widget(
        BrailleChart::new(&data, max, &app.theme.power, app.theme.graph_text).block(block),
        area,
    );
}

fn draw_slots_panel(frame: &mut Frame, area: Rect, app: &App, focused: bool) {
    let colors = app.theme.colors();
    let busy = app
        .snapshot
        .slots
        .iter()
        .filter(|slot| slot.is_processing)
        .count();
    let configured_total = app
        .snapshot
        .props
        .as_ref()
        .map(|props| props.total_slots.max(0) as usize)
        .unwrap_or(0);
    let total = configured_total.max(app.snapshot.slots.len());
    let speculative = app.snapshot.slots.iter().any(|slot| slot.speculative);
    let title = if speculative {
        "SLOTS · SPEC READY"
    } else {
        "SLOTS"
    };
    let queued = app.snapshot.metrics.requests_deferred.max(0.0);
    let (summary, summary_color) = if app.snapshot.slots_error.is_some() {
        ("telemetry unavailable ".to_string(), colors.error)
    } else if area.width >= 62 {
        (
            format!("{busy}/{total} busy · {queued:.0} queued "),
            if queued > 0.0 {
                colors.warn
            } else if busy > 0 {
                colors.ok
            } else {
                colors.dim
            },
        )
    } else {
        (
            format!("{busy}/{total} busy "),
            if busy > 0 { colors.ok } else { colors.dim },
        )
    };
    let block = panel_block(
        panel_title(title, colors.title),
        panel_value(summary, summary_color),
        focused,
        app.theme.slots_border,
        colors,
    );
    let inner = block.inner(area);
    block.render(area, frame.buffer_mut());

    if app.snapshot.slots.is_empty() {
        let message = if let Some(error) = app.snapshot.slots_error.as_deref() {
            error
        } else if !app.snapshot.connected {
            "Server unreachable"
        } else {
            "No slots reported"
        };
        draw_empty_state(frame, inner, message, colors);
        return;
    }

    let row_budget = inner.height.saturating_sub(2) as usize;
    let max_offset = app.snapshot.slots.len().saturating_sub(row_budget.max(1));
    let offset = (app.scroll as usize).min(max_offset);
    let full_table = inner.width >= 58;
    let rows: Vec<Row> = app
        .snapshot
        .slots
        .iter()
        .skip(offset)
        .take(row_budget)
        .map(|slot| {
            let state = if slot.is_processing {
                Line::from(Span::styled("● busy", Style::default().fg(colors.ok)))
            } else {
                Line::from(Span::styled("○ idle", Style::default().fg(colors.dim)))
            };
            let id = Line::from(Span::styled(
                format!("{:02}", slot.id),
                Style::default()
                    .fg(colors.bright)
                    .add_modifier(Modifier::BOLD),
            ));
            let context_percent = if slot.context_capacity > 0 {
                slot.context_tokens.max(0) as f64 / slot.context_capacity as f64 * 100.0
            } else {
                0.0
            };
            let context_label = if slot.context_capacity > 0 {
                format!(
                    "{}/{} {:.0}%",
                    fmt_num(slot.context_tokens.max(0) as f64),
                    fmt_num(slot.context_capacity as f64),
                    context_percent
                )
            } else {
                format!("{} / —", fmt_num(slot.context_tokens.max(0) as f64))
            };
            let context = Line::from(Span::styled(
                context_label,
                Style::default().fg(app.theme.memory.at(context_percent)),
            ))
            .right_aligned();
            let output = if slot.is_processing {
                Line::from(Span::styled(
                    fmt_num(slot.current_output_tokens() as f64),
                    Style::default().fg(colors.predict),
                ))
                .right_aligned()
            } else {
                Line::from(Span::styled("—", Style::default().fg(colors.dim))).right_aligned()
            };

            if full_table {
                let evaluated = if slot.is_processing {
                    Line::from(Span::styled(
                        fmt_num(slot.prompt_tokens_processed.max(0) as f64),
                        Style::default().fg(colors.prompt),
                    ))
                    .right_aligned()
                } else {
                    Line::from(Span::styled("—", Style::default().fg(colors.dim))).right_aligned()
                };
                let accounted_prompt =
                    slot.prompt_tokens_cached.max(0) + slot.prompt_tokens_processed.max(0);
                let cached = if slot.is_processing && accounted_prompt > 0 {
                    let cached_percent =
                        slot.prompt_tokens_cached.max(0) as f64 / accounted_prompt as f64 * 100.0;
                    Line::from(Span::styled(
                        format!("{cached_percent:.0}%"),
                        Style::default().fg(app.theme.cache.at(cached_percent)),
                    ))
                    .right_aligned()
                } else {
                    Line::from(Span::styled("—", Style::default().fg(colors.dim))).right_aligned()
                };
                Row::new(vec![id, state, context, evaluated, cached, output])
            } else {
                Row::new(vec![id, state, context, output])
            }
        })
        .collect();

    let (header, widths) = if full_table {
        (
            Row::new(vec![
                Line::from("ID"),
                Line::from("STATE"),
                Line::from("CONTEXT").right_aligned(),
                Line::from("EVAL").right_aligned(),
                Line::from("CACHED").right_aligned(),
                Line::from("OUTPUT").right_aligned(),
            ]),
            vec![
                Constraint::Length(3),
                Constraint::Length(8),
                Constraint::Min(18),
                Constraint::Length(7),
                Constraint::Length(8),
                Constraint::Length(7),
            ],
        )
    } else {
        (
            Row::new(vec![
                Line::from("ID"),
                Line::from("STATE"),
                Line::from("CONTEXT").right_aligned(),
                Line::from("OUTPUT").right_aligned(),
            ]),
            vec![
                Constraint::Length(3),
                Constraint::Length(8),
                Constraint::Min(17),
                Constraint::Length(7),
            ],
        )
    };

    let header = header
        .style(Style::default().fg(colors.dim).add_modifier(Modifier::BOLD))
        .bottom_margin(u16::from(inner.height >= 5));
    frame.render_widget(
        Table::new(rows, widths).header(header).column_spacing(1),
        inner,
    );
}

fn draw_gpu_panel(frame: &mut Frame, area: Rect, app: &App, focused: bool) {
    let colors = app.theme.colors();
    let window = gpu_panel_window(
        area.height.saturating_sub(2) as usize,
        app.snapshot.gpus.len(),
        app.scroll as usize,
    );
    let (memory_used, memory_total) = gpu_memory_totals(&app.snapshot.gpus);
    let total_memory_pct = memory_percent(memory_used, memory_total);
    let block = panel_block(
        panel_title("GPU MEMORY", colors.title),
        panel_value(
            gpu_memory_summary(
                memory_used,
                memory_total,
                area.width,
                window,
                app.snapshot.gpus.len(),
            ),
            if memory_total > 0 {
                app.theme.memory.at(total_memory_pct)
            } else {
                colors.dim
            },
        ),
        focused,
        app.theme.memory_border,
        colors,
    );
    let inner = block.inner(area);
    block.render(area, frame.buffer_mut());

    if app.snapshot.gpus.is_empty() {
        draw_empty_state(frame, inner, "GPU telemetry unavailable", colors);
        return;
    }

    let mut y = inner.y;
    for (visible_index, gpu) in app
        .snapshot
        .gpus
        .iter()
        .skip(window.offset)
        .take(window.visible)
        .enumerate()
    {
        if y >= inner.bottom() {
            break;
        }

        let memory_pct = memory_percent(gpu.mem_used, gpu.mem_total);
        let memory_color = app.theme.memory.at(memory_pct);
        let temperature_color = app.theme.temperature.at(gpu.temp.clamp(0.0, 100.0));
        let gpu_color = app.theme.gpu.at(gpu.gpu_util);
        let power_pct = if gpu.power_limit > 0.0 {
            gpu.power_draw / gpu.power_limit * 100.0
        } else {
            0.0
        };
        let power_color = if gpu.power_limit > 0.0 {
            app.theme.power.at(power_pct)
        } else {
            colors.power
        };

        if window.lines_per_gpu == 1 {
            let memory_value = compact_memory_pair(gpu.mem_used, gpu.mem_total);
            let percent = memory_percent_label(gpu.mem_total, memory_pct);
            let id = format!("GPU {}", gpu.index);
            let fixed_width =
                id.chars().count() + memory_value.chars().count() + percent.chars().count() + 3;
            let bar_width = (inner.width as usize).saturating_sub(fixed_width);
            let mut spans = vec![Span::styled(
                id,
                Style::default()
                    .fg(colors.bright)
                    .add_modifier(Modifier::BOLD),
            )];
            spans.push(Span::raw(" "));
            spans.extend(gradient_progress_bar(
                memory_pct,
                bar_width,
                &app.theme.memory,
                colors.track,
            ));
            spans.push(Span::styled(
                format!(" {percent} "),
                Style::default()
                    .fg(memory_color)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                memory_value,
                Style::default().fg(memory_color),
            ));
            frame.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect::new(inner.x, y, inner.width, 1),
            );
            y += 1;
            y = y
                .saturating_add(gpu_gap_after(visible_index, window, inner.height as usize) as u16)
                .min(inner.bottom());
            continue;
        }

        let memory_value = if inner.width >= 64 {
            full_memory_pair(gpu.mem_used, gpu.mem_total)
        } else {
            compact_memory_pair(gpu.mem_used, gpu.mem_total)
        };
        let value_width = memory_value.chars().count().min(inner.width as usize) as u16;
        let identity_width = inner.width.saturating_sub(value_width + 1);
        let identity_area = Rect::new(inner.x, y, identity_width, 1);
        let value_area = Rect::new(inner.right().saturating_sub(value_width), y, value_width, 1);
        frame.render_widget(
            Paragraph::new(gpu_identity_line(
                gpu,
                identity_width as usize,
                window.lines_per_gpu,
                gpu_color,
                temperature_color,
                power_color,
                colors,
            )),
            identity_area,
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                memory_value,
                Style::default()
                    .fg(memory_color)
                    .add_modifier(Modifier::BOLD),
            )))
            .alignment(Alignment::Right),
            value_area,
        );
        y += 1;

        if y < inner.bottom() {
            let percent = memory_percent_label(gpu.mem_total, memory_pct);
            let percent_width = percent.chars().count();
            let bar_width = (inner.width as usize).saturating_sub(percent_width + 2);
            let mut spans =
                gradient_progress_bar(memory_pct, bar_width, &app.theme.memory, colors.track);
            spans.push(Span::styled(
                format!("  {percent}"),
                Style::default()
                    .fg(memory_color)
                    .add_modifier(Modifier::BOLD),
            ));
            frame.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect::new(inner.x, y, inner.width, 1),
            );
            y += 1;
        }

        if window.lines_per_gpu == 3 && y < inner.bottom() {
            frame.render_widget(
                Paragraph::new(gpu_telemetry_line(
                    gpu,
                    gpu_color,
                    temperature_color,
                    power_color,
                    colors,
                )),
                Rect::new(inner.x, y, inner.width, 1),
            );
            y += 1;
        }
        y = y
            .saturating_add(gpu_gap_after(visible_index, window, inner.height as usize) as u16)
            .min(inner.bottom());
    }
}

fn draw_stats_panel(frame: &mut Frame, area: Rect, app: &App, focused: bool) {
    let colors = app.theme.colors();
    let block = panel_block(
        panel_title("SESSION", colors.title),
        panel_value("live totals ".to_string(), colors.dim),
        focused,
        colors.border,
        colors,
    );
    let inner = block.inner(area);
    block.render(area, frame.buffer_mut());

    let metrics = &app.snapshot.metrics;
    let total_slots = app
        .snapshot
        .props
        .as_ref()
        .map(|props| props.total_slots)
        .unwrap_or(app.snapshot.slots.len() as i64);
    let speculative_slots = app
        .snapshot
        .slots
        .iter()
        .filter(|slot| slot.speculative)
        .count();
    let tokens_per_decode = if metrics.n_decode_total > 0.0 {
        metrics.tokens_predicted_total / metrics.n_decode_total
    } else {
        0.0
    };

    let values = vec![
        match app.prompt_rate_basis {
            PromptRateBasis::Interval => (
                "LAST PP",
                format!("{:.1} tok/s", app.prompt_rate),
                colors.prompt,
            ),
            PromptRateBasis::ServerAverage => (
                "PP AVG",
                format!("{:.1} tok/s", app.prompt_rate),
                colors.prompt,
            ),
            PromptRateBasis::Unavailable => ("PP SPEED", "—".to_string(), colors.prompt),
        },
        (
            "GENERATE",
            format!("{:.1} tok/s", app.predict_rate),
            colors.predict,
        ),
        (
            "ACTIVE",
            format!("{:.0} / {total_slots}", metrics.requests_processing),
            if metrics.requests_processing > 0.0 {
                app.theme
                    .process
                    .at(metrics.requests_processing / total_slots.max(1) as f64 * 100.0)
            } else {
                colors.text
            },
        ),
        (
            "QUEUED",
            format!("{:.0}", metrics.requests_deferred),
            if metrics.requests_deferred > 0.0 {
                colors.warn
            } else {
                colors.text
            },
        ),
        ("EVAL TOK", fmt_num(app.total_prompt_tokens), colors.prompt),
        (
            "GENERATED",
            fmt_num(app.total_predict_tokens),
            colors.predict,
        ),
        ("DECODES", fmt_num(metrics.n_decode_total), colors.bright),
        (
            "TOK/DECODE",
            format!("{tokens_per_decode:.2}"),
            if tokens_per_decode >= 1.2 {
                colors.ok
            } else {
                colors.text
            },
        ),
        (
            "SPEC READY",
            if speculative_slots > 0 {
                format!("{speculative_slots}/{total_slots} slots")
            } else {
                "off".to_string()
            },
            if speculative_slots > 0 {
                colors.ok
            } else {
                colors.dim
            },
        ),
        (
            "BUSY/DECODE",
            format!("{:.1}", metrics.n_busy_slots_per_decode),
            colors.text,
        ),
    ];

    draw_metric_grid(frame, inner, &values, colors);
}

fn draw_metric_grid(
    frame: &mut Frame,
    area: Rect,
    metrics: &[(&'static str, String, Color)],
    colors: ThemeColors,
) {
    if area.is_empty() {
        return;
    }

    let column_count = if area.width >= 42 { 2 } else { 1 };
    let column_width = area.width / column_count;
    for (index, (label, value, color)) in metrics.iter().enumerate() {
        let row = index as u16 / column_count;
        if row >= area.height {
            break;
        }
        let column = index as u16 % column_count;
        let x = area.x + column * column_width;
        let width = if column + 1 == column_count {
            area.right().saturating_sub(x)
        } else {
            column_width.saturating_sub(1)
        };
        let cell = Rect::new(x, area.y + row, width, 1);
        frame.render_widget(
            Paragraph::new(Span::styled(
                *label,
                Style::default().fg(colors.dim).add_modifier(Modifier::BOLD),
            )),
            cell,
        );
        frame.render_widget(
            Paragraph::new(Span::styled(
                value.clone(),
                Style::default().fg(*color).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Right),
            cell,
        );
    }
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let colors = app.theme.colors();
    let scroll_hint =
        matches!(app.current_section, Section::Slots | Section::Gpu) && area.width >= 78;
    let show_interval_control = area.width >= 100;
    let controls_width = (if scroll_hint && app.paused {
        54
    } else if scroll_hint {
        52
    } else if app.paused {
        43
    } else {
        34
    }) + u16::from(show_interval_control) * 10;
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(24), Constraint::Length(controls_width)])
        .split(area);
    let full_labels = chunks[0].width >= 44;

    let mut tabs = vec![
        Span::raw(" "),
        Span::styled("Tab", key_style(colors)),
        Span::styled("  ", Style::default().fg(colors.dim)),
    ];
    for (section, long, short) in [
        (Section::Overview, "Overview", "OVR"),
        (Section::Throughput, "Throughput", "RATE"),
        (Section::Slots, "Slots", "SLOT"),
        (Section::Gpu, "GPU", "GPU"),
    ] {
        let selected = app.current_section == section;
        tabs.push(Span::styled(
            format!(" {} ", if full_labels { long } else { short }),
            if selected {
                Style::default()
                    .fg(colors.selected_fg)
                    .bg(colors.selected_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors.dim)
            },
        ));
        tabs.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(tabs)), chunks[0]);

    let mut controls = Vec::new();
    if show_interval_control {
        controls.push(Span::styled("-/+", key_style(colors)));
        controls.push(Span::styled(
            format!(" {}  ", app.update_interval_label()),
            Style::default().fg(colors.dim),
        ));
    }
    if scroll_hint {
        controls.push(Span::styled("↑↓", key_style(colors)));
        controls.push(Span::styled(" scroll  ", Style::default().fg(colors.dim)));
    }
    if app.paused {
        controls.push(Span::styled(
            "■ PAUSED  ",
            Style::default()
                .fg(colors.warn)
                .add_modifier(Modifier::BOLD),
        ));
        controls.push(Span::styled("p", key_style(colors)));
        controls.push(Span::styled(" resume  ", Style::default().fg(colors.dim)));
    } else {
        controls.push(Span::styled("p", key_style(colors)));
        controls.push(Span::styled(" pause  ", Style::default().fg(colors.dim)));
    }
    controls.push(Span::styled("t", key_style(colors)));
    controls.push(Span::styled(" theme  ", Style::default().fg(colors.dim)));
    controls.push(Span::styled("?", key_style(colors)));
    controls.push(Span::styled(" help  ", Style::default().fg(colors.dim)));
    controls.push(Span::styled("q", key_style(colors)));
    controls.push(Span::styled(" quit", Style::default().fg(colors.dim)));
    frame.render_widget(
        Paragraph::new(Line::from(controls)).alignment(Alignment::Right),
        chunks[1],
    );
}

fn draw_help(frame: &mut Frame, area: Rect, app: &App) {
    let colors = app.theme.colors();
    let popup = centered_fixed(area, area.width.min(78), area.height.min(25));
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors.border_highlight))
        .style(Style::default().bg(if app.theme_background {
            colors.surface
        } else {
            Color::Reset
        }))
        .padding(Padding::new(2, 2, 1, 1))
        .title(Line::from(vec![
            Span::styled(
                BRAND_TITLE,
                Style::default()
                    .fg(colors.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("quick help ", Style::default().fg(colors.dim)),
        ]))
        .title_bottom(
            Line::from(Span::styled(
                " ? / h  close ",
                Style::default().fg(colors.dim),
            ))
            .right_aligned(),
        );
    let inner = block.inner(popup);
    block.render(popup, frame.buffer_mut());

    let help = vec![
        Line::from(vec![
            Span::styled(
                "KEYBOARD",
                Style::default()
                    .fg(colors.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  Navigate and control sampling",
                Style::default().fg(colors.dim),
            ),
        ]),
        Line::from(""),
        help_row("Tab / Shift-Tab", "Next / previous section", colors),
        help_row("↑ / ↓", "Scroll slot and GPU lists", colors),
        help_row("p", "Pause / resume data collection", colors),
        help_row("- / +", "Faster / slower polling by 100 ms", colors),
        help_row("t", "Preview and choose a theme", colors),
        help_row("? / h", "Open / close this help", colors),
        help_row("q / Ctrl-C", "Quit ltop", colors),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "DATA SOURCES",
                Style::default()
                    .fg(colors.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "  One {} cadence for llama.cpp and the host",
                    app.update_interval_label()
                ),
                Style::default().fg(colors.dim),
            ),
        ]),
        Line::from(""),
        help_row(
            "/metrics",
            "Server-timed prompt eval, token totals, and requests",
            colors,
        ),
        help_row(
            "/slots",
            "Lane activity, context use, evaluated/cached input, and output",
            colors,
        ),
        help_row("/props", "Model, build, slots, and context size", colors),
        help_row(
            "nvidia-smi",
            "GPU load, memory, temperature, and power",
            colors,
        ),
        Line::from(""),
        Line::from(Span::styled(
            "Pass a URL to override auto-detection:  ltop http://host:port",
            Style::default().fg(colors.text),
        )),
    ];
    frame.render_widget(
        Paragraph::new(help)
            .style(Style::default().bg(if app.theme_background {
                colors.surface
            } else {
                Color::Reset
            }))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn draw_theme_picker(frame: &mut Frame, area: Rect, app: &App) {
    let colors = app.theme.colors();
    let width = area.width.saturating_sub(4).min(72);
    let desired_height = app.theme_count().saturating_add(6) as u16;
    let height = desired_height.min(area.height.saturating_sub(4)).max(9);
    let popup = centered_fixed(area, width, height);
    frame.render_widget(Clear, popup);

    let surface = if app.theme_background {
        colors.surface
    } else {
        Color::Reset
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors.border_highlight))
        .style(Style::default().fg(colors.text).bg(surface))
        .padding(Padding::horizontal(1))
        .title(Line::from(vec![
            Span::styled(
                " THEME ",
                Style::default()
                    .fg(colors.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("· {} ", app.theme.name()),
                Style::default().fg(colors.title),
            ),
        ]));
    let inner = block.inner(popup);
    block.render(popup, frame.buffer_mut());

    if inner.height < 3 {
        return;
    }

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("●", Style::default().fg(colors.accent)),
            Span::styled(
                " applied · › preview · btop .theme files",
                Style::default().fg(colors.dim),
            ),
        ])),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    let visible = inner.height.saturating_sub(2) as usize;
    let selected = app.picker_theme_index();
    let start = selected
        .saturating_sub(visible / 2)
        .min(app.theme_count().saturating_sub(visible));
    for row_index in 0..visible {
        let theme_index = start + row_index;
        if theme_index >= app.theme_count() {
            break;
        }
        let row = Rect::new(inner.x, inner.y + 1 + row_index as u16, inner.width, 1);
        let is_selected = theme_index == selected;
        let is_active = theme_index == app.active_theme_index();
        let row_background = if is_selected {
            colors.selected_bg
        } else {
            surface
        };
        let row_style = if is_selected {
            Style::default()
                .fg(colors.selected_fg)
                .bg(row_background)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors.text).bg(row_background)
        };
        frame.render_widget(Block::default().style(row_style), row);

        let swatch_width = if row.width >= 28 { 9 } else { 0 };
        let name_width = row.width.saturating_sub(swatch_width);
        let name = truncate_chars(
            app.theme_name(theme_index),
            name_width.saturating_sub(3) as usize,
        );
        let marker = if is_active {
            "●"
        } else if is_selected {
            "›"
        } else {
            " "
        };
        frame.render_widget(
            Paragraph::new(format!("{marker} {name}")).style(row_style),
            Rect::new(row.x, row.y, name_width, 1),
        );

        if swatch_width > 0 {
            let preview = app.theme_at(theme_index).colors();
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("■", Style::default().fg(preview.prompt).bg(row_background)),
                    Span::raw(" "),
                    Span::styled("■", Style::default().fg(preview.predict).bg(row_background)),
                    Span::raw(" "),
                    Span::styled("■", Style::default().fg(preview.gpu).bg(row_background)),
                    Span::raw(" "),
                    Span::styled("■", Style::default().fg(preview.memory).bg(row_background)),
                ]))
                .style(row_style),
                Rect::new(
                    row.right().saturating_sub(swatch_width),
                    row.y,
                    swatch_width,
                    1,
                ),
            );
        }
    }

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("↑↓", key_style(colors)),
            Span::styled(" preview  ", Style::default().fg(colors.dim)),
            Span::styled("b", key_style(colors)),
            Span::styled(
                if app.theme_background {
                    " bg:on  "
                } else {
                    " bg:off  "
                },
                Style::default().fg(colors.dim),
            ),
            Span::styled("Enter", key_style(colors)),
            Span::styled(" apply  ", Style::default().fg(colors.dim)),
            Span::styled("Esc", key_style(colors)),
            Span::styled(" cancel", Style::default().fg(colors.dim)),
        ]))
        .alignment(Alignment::Center),
        Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
    );
}

fn draw_too_small(frame: &mut Frame, area: Rect, colors: ThemeColors) {
    let width = area.width.saturating_sub(2).min(50);
    let height = area.height.saturating_sub(2).min(7);
    let popup = centered_fixed(area, width, height);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors.border_highlight))
        .title(Span::styled(
            BRAND_TITLE,
            Style::default()
                .fg(colors.accent)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    block.render(popup, frame.buffer_mut());
    let message = vec![
        Line::from(Span::styled(
            "Terminal too small",
            Style::default()
                .fg(colors.bright)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            format!(
                "Need at least {MIN_WIDTH}×{MIN_HEIGHT} · now {}×{}",
                area.width, area.height
            ),
            Style::default().fg(colors.dim),
        )),
        Line::from(Span::styled(
            "Resize or press q to quit",
            Style::default().fg(colors.text),
        )),
    ];
    let y = inner.y + inner.height.saturating_sub(message.len() as u16) / 2;
    frame.render_widget(
        Paragraph::new(message).alignment(Alignment::Center),
        Rect::new(inner.x, y, inner.width, inner.bottom().saturating_sub(y)),
    );
}

fn panel_block<'a>(
    title: Line<'a>,
    value: Line<'a>,
    focused: bool,
    border: Color,
    colors: ThemeColors,
) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused {
            colors.border_highlight
        } else {
            border
        }))
        .padding(Padding::horizontal(1))
        .title(title)
        .title(value.right_aligned())
}

fn panel_title(name: &'static str, color: Color) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {name} "),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    ))
}

fn panel_value(value: String, color: Color) -> Line<'static> {
    Line::from(Span::styled(
        value,
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    ))
}

fn label_span(label: &'static str, colors: ThemeColors) -> Span<'static> {
    Span::styled(
        label,
        Style::default().fg(colors.dim).add_modifier(Modifier::BOLD),
    )
}

fn value_style(colors: ThemeColors) -> Style {
    Style::default()
        .fg(colors.bright)
        .add_modifier(Modifier::BOLD)
}

fn key_style(colors: ThemeColors) -> Style {
    Style::default()
        .fg(colors.bright)
        .add_modifier(Modifier::BOLD)
}

fn connection_status(app: &App) -> (&'static str, Color) {
    let colors = app.theme.colors();
    if app.paused {
        ("PAUSED", colors.warn)
    } else if !app.snapshot.connected {
        ("OFFLINE", colors.error)
    } else if app.snapshot.error.is_some() {
        ("DEGRADED", colors.warn)
    } else if app
        .snapshot
        .props
        .as_ref()
        .is_some_and(|props| props.is_sleeping)
    {
        ("SLEEPING", colors.dim)
    } else if app.snapshot.metrics.requests_processing > 0.0
        || app.snapshot.slots.iter().any(|slot| slot.is_processing)
    {
        ("ACTIVE", colors.ok)
    } else {
        ("READY", colors.ok)
    }
}

fn hardware_summary(app: &App) -> String {
    if app.snapshot.gpus.is_empty() {
        return "No GPU telemetry".to_string();
    }
    let used: u64 = app.snapshot.gpus.iter().map(|gpu| gpu.mem_used).sum();
    let total: u64 = app.snapshot.gpus.iter().map(|gpu| gpu.mem_total).sum();
    let name = app
        .snapshot
        .gpus
        .first()
        .map(|gpu| shortened_gpu_name(&gpu.name))
        .unwrap_or_default();
    format!(
        "{} × {} · {} / {}",
        app.snapshot.gpus.len(),
        name,
        fmt_memory(used),
        fmt_memory(total)
    )
}

fn shortened_gpu_name(name: &str) -> String {
    let shortened = name
        .strip_prefix("NVIDIA ")
        .unwrap_or(name)
        .strip_prefix("GeForce ")
        .unwrap_or_else(|| name.strip_prefix("NVIDIA ").unwrap_or(name));
    let mut chars = shortened.chars();
    let visible: String = chars.by_ref().take(20).collect();
    if chars.next().is_some() {
        format!("{visible}…")
    } else {
        visible
    }
}

fn average_gpu_util(app: &App) -> f64 {
    if app.snapshot.gpus.is_empty() {
        0.0
    } else {
        app.snapshot
            .gpus
            .iter()
            .map(|gpu| gpu.gpu_util)
            .sum::<f64>()
            / app.snapshot.gpus.len() as f64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GpuPanelWindow {
    lines_per_gpu: usize,
    visible: usize,
    offset: usize,
    end: usize,
}

fn gpu_panel_window(height: usize, gpu_count: usize, requested_offset: usize) -> GpuPanelWindow {
    if gpu_count == 0 || height == 0 {
        return GpuPanelWindow {
            lines_per_gpu: 1,
            visible: 0,
            offset: 0,
            end: 0,
        };
    }

    let lines_per_gpu = if height >= gpu_count.saturating_mul(3) {
        3
    } else if height >= 2 {
        2
    } else {
        1
    };
    let visible = (height / lines_per_gpu).max(1).min(gpu_count);
    let offset = requested_offset.min(gpu_count.saturating_sub(visible));

    GpuPanelWindow {
        lines_per_gpu,
        visible,
        offset,
        end: (offset + visible).min(gpu_count),
    }
}

fn gpu_gap_after(index: usize, window: GpuPanelWindow, height: usize) -> usize {
    if index + 1 >= window.visible || window.visible <= 1 {
        return 0;
    }

    let gap_slots = window.visible - 1;
    let gap_budget = height
        .saturating_sub(window.visible.saturating_mul(window.lines_per_gpu))
        .min(gap_slots);
    let rounded_share = |position: usize| {
        position
            .saturating_mul(gap_budget)
            .saturating_add(gap_slots / 2)
            / gap_slots
    };
    rounded_share(index + 1).saturating_sub(rounded_share(index))
}

fn gpu_memory_totals(gpus: &[GpuInfo]) -> (u64, u64) {
    gpus.iter().fold((0, 0), |(used, total), gpu| {
        (
            used.saturating_add(gpu.mem_used),
            total.saturating_add(gpu.mem_total),
        )
    })
}

fn memory_percent(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        used as f64 / total as f64 * 100.0
    }
}

fn memory_percent_label(total: u64, percent: f64) -> String {
    if total == 0 {
        "—".to_string()
    } else {
        format!("{percent:.0}%")
    }
}

fn full_memory_pair(used: u64, total: u64) -> String {
    if total == 0 {
        return "memory unavailable".to_string();
    }

    if total >= 1024 {
        format!(
            "{:.1} / {:.1} GiB",
            used as f64 / 1024.0,
            total as f64 / 1024.0
        )
    } else {
        format!("{used} / {total} MiB")
    }
}

fn compact_memory_pair(used: u64, total: u64) -> String {
    if total == 0 {
        return "—".to_string();
    }

    if total >= 1024 {
        format!("{:.1}/{:.1}G", used as f64 / 1024.0, total as f64 / 1024.0)
    } else {
        format!("{used}/{total}M")
    }
}

fn gpu_memory_summary(
    used: u64,
    total: u64,
    panel_width: u16,
    window: GpuPanelWindow,
    gpu_count: usize,
) -> String {
    if gpu_count == 0 {
        return "waiting ".to_string();
    }

    let percent = memory_percent(used, total);
    let range = if window.visible < gpu_count {
        format!(" · {}–{}/{}", window.offset + 1, window.end, gpu_count)
    } else {
        String::new()
    };
    let full = format!(
        "{} · {}{}",
        full_memory_pair(used, total),
        memory_percent_label(total, percent),
        range
    );
    let compact = format!(
        "{} · {}{}",
        compact_memory_pair(used, total),
        memory_percent_label(total, percent),
        range
    );
    let minimal = format!("{}{}", memory_percent_label(total, percent), range);
    // Account for the left title, borders, and a little separation between
    // the independently aligned block titles.
    let available = panel_width.saturating_sub(15) as usize;
    let summary = [full, compact, minimal]
        .into_iter()
        .find(|candidate| candidate.chars().count() <= available)
        .unwrap_or_else(|| format!("{gpu_count} GPUs"));
    format!("{summary} ")
}

fn gpu_identity_line(
    gpu: &GpuInfo,
    width: usize,
    lines_per_gpu: usize,
    gpu_color: Color,
    temperature_color: Color,
    power_color: Color,
    colors: ThemeColors,
) -> Line<'static> {
    let id = format!("GPU {}", gpu.index);
    let name = shortened_gpu_name(&gpu.name);
    let utilization = format!("{:.0}%", gpu.gpu_util);
    let temperature = format!("{:.0}°C", gpu.temp);
    let power = format!("{:.0}W", gpu.power_draw);
    let name_width = usize::from(!name.is_empty()) * (name.chars().count() + 2);
    let details_width =
        utilization.chars().count() + temperature.chars().count() + power.chars().count() + 9;
    let base_width = id.chars().count();
    let show_name = !name.is_empty()
        && (lines_per_gpu == 3
            || base_width + name_width + details_width <= width
            || base_width + name_width <= width
                && details_width > width.saturating_sub(base_width));
    let show_details = lines_per_gpu == 2
        && base_width + details_width + if show_name { name_width } else { 0 } <= width;

    let mut spans = vec![Span::styled(
        id,
        Style::default()
            .fg(colors.bright)
            .add_modifier(Modifier::BOLD),
    )];
    if show_name {
        spans.push(Span::styled(
            format!("  {name}"),
            Style::default().fg(colors.dim),
        ));
    }
    if show_details {
        spans.push(Span::styled("   ", Style::default().fg(colors.dim)));
        spans.push(Span::styled(utilization, Style::default().fg(gpu_color)));
        spans.push(Span::styled(" · ", Style::default().fg(colors.dim)));
        spans.push(Span::styled(
            temperature,
            Style::default().fg(temperature_color),
        ));
        spans.push(Span::styled(" · ", Style::default().fg(colors.dim)));
        spans.push(Span::styled(power, Style::default().fg(power_color)));
    }
    Line::from(spans)
}

fn gpu_telemetry_line(
    gpu: &GpuInfo,
    gpu_color: Color,
    temperature_color: Color,
    power_color: Color,
    colors: ThemeColors,
) -> Line<'static> {
    let power = if gpu.power_limit > 0.0 {
        format!("{:.0}/{:.0}W", gpu.power_draw, gpu.power_limit)
    } else {
        format!("{:.0}W", gpu.power_draw)
    };
    Line::from(vec![
        label_span("UTIL ", colors),
        Span::styled(
            format!("{:.0}%", gpu.gpu_util),
            Style::default().fg(gpu_color),
        ),
        label_span("   TEMP ", colors),
        Span::styled(
            format!("{:.0}°C", gpu.temp),
            Style::default().fg(temperature_color),
        ),
        label_span("   POWER ", colors),
        Span::styled(power, Style::default().fg(power_color)),
    ])
}

fn gradient_progress_bar(
    percent: f64,
    width: usize,
    gradient: &Gradient,
    track: Color,
) -> Vec<Span<'static>> {
    const PARTIAL: [char; 8] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉'];

    let exact = percent.clamp(0.0, 100.0) / 100.0 * width as f64;
    let full = exact.floor() as usize;
    let fraction = ((exact - full as f64) * 8.0).floor() as usize;
    let has_partial = fraction > 0 && full < width;
    let empty = width.saturating_sub(full + usize::from(has_partial));
    let mut spans = Vec::with_capacity(full + usize::from(has_partial) + usize::from(empty > 0));

    for column in 0..full {
        let capacity_percent = (column + 1) as f64 / width.max(1) as f64 * 100.0;
        spans.push(Span::styled(
            "█",
            Style::default().fg(gradient.at(capacity_percent)),
        ));
    }
    if has_partial {
        let capacity_percent = (full + 1) as f64 / width.max(1) as f64 * 100.0;
        spans.push(Span::styled(
            PARTIAL[fraction].to_string(),
            Style::default().fg(gradient.at(capacity_percent)),
        ));
    }
    if empty > 0 {
        spans.push(Span::styled("░".repeat(empty), Style::default().fg(track)));
    }
    spans
}

fn draw_empty_state(frame: &mut Frame, area: Rect, message: &str, colors: ThemeColors) {
    if area.is_empty() {
        return;
    }
    let y = area.y + area.height / 2;
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("○  ", Style::default().fg(colors.dim)),
            Span::styled(message.to_string(), Style::default().fg(colors.dim)),
        ]))
        .alignment(Alignment::Center),
        Rect::new(area.x, y, area.width, 1),
    );
}

fn help_row(key: &'static str, description: &'static str, colors: ThemeColors) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{key:<18}"),
            Style::default()
                .fg(colors.bright)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(description, Style::default().fg(colors.text)),
    ])
}

fn truncate_chars(value: &str, width: usize) -> String {
    if value.chars().count() <= width {
        return value.to_string();
    }
    if width <= 1 {
        return "…".chars().take(width).collect();
    }
    let mut shortened: String = value.chars().take(width - 1).collect();
    shortened.push('…');
    shortened
}

fn centered_fixed(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn fmt_num(number: f64) -> String {
    if number.abs() >= 1_000_000_000.0 {
        format!("{:.1}B", number / 1_000_000_000.0)
    } else if number.abs() >= 1_000_000.0 {
        format!("{:.1}M", number / 1_000_000.0)
    } else if number.abs() >= 1_000.0 {
        format!("{:.1}K", number / 1_000.0)
    } else {
        format!("{number:.0}")
    }
}

fn fmt_memory(mib: u64) -> String {
    if mib >= 1024 {
        format!("{:.1} GiB", mib as f64 / 1024.0)
    } else {
        format!("{mib} MiB")
    }
}

fn fmt_slot_count(count: i64) -> String {
    format!("{count} {}", if count == 1 { "slot" } else { "slots" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{GpuInfo, Metrics, ServerProps, SlotInfo};
    use crate::theme::ThemeCatalog;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn demo_app() -> App {
        let mut app = App::new("http://127.0.0.1:8080".to_string());
        app.snapshot.connected = true;
        app.snapshot.metrics = Metrics {
            prompt_tokens_total: 70_700.0,
            tokens_predicted_total: 28_800.0,
            n_decode_total: 18_400.0,
            n_tokens_max: 4096.0,
            requests_processing: 1.0,
            requests_deferred: 2.0,
            n_busy_slots_per_decode: 1.4,
            ..Metrics::default()
        };
        app.snapshot.props = Some(ServerProps {
            model_alias: "deepseek-v4-flash-iq4".to_string(),
            model_ftype: "IQ4_XS".to_string(),
            total_slots: 2,
            n_ctx: 262_144,
            build_info: "b6100".to_string(),
            ..ServerProps::default()
        });
        app.snapshot.slots = vec![
            SlotInfo {
                id: 0,
                task_id: Some(42),
                context_capacity: 262_144,
                speculative: true,
                is_processing: true,
                context_tokens: 12_840,
                prompt_tokens_processed: 2_200,
                prompt_tokens_cached: 10_240,
                decoded_tokens: 400,
            },
            SlotInfo {
                id: 1,
                task_id: Some(41),
                context_capacity: 262_144,
                speculative: true,
                context_tokens: 90_218,
                prompt_tokens_processed: 2_429,
                decoded_tokens: 321,
                ..SlotInfo::default()
            },
        ];
        app.snapshot.gpus = (0..4)
            .map(|index| GpuInfo {
                index,
                name: "NVIDIA GeForce RTX 4090".to_string(),
                gpu_util: 42.0 + index as f64 * 3.0,
                mem_used: 20_500 + index as u64 * 300,
                mem_total: 24_564,
                temp: 58.0 + index as f64,
                power_draw: 255.0 + index as f64 * 8.0,
                power_limit: 450.0,
                ..GpuInfo::default()
            })
            .collect();
        app.prompt_rate = 358.8;
        app.prompt_rate_basis = PromptRateBasis::Interval;
        app.predict_rate = 49.1;
        app.total_prompt_tokens = 70_700.0;
        app.total_predict_tokens = 28_800.0;
        for sample in 0..60 {
            let wave = ((sample as f64 / 7.0).sin() + 1.0) / 2.0;
            app.prompt_rate_history.push_back(250.0 + wave * 140.0);
            app.predict_rate_history.push_back(36.0 + wave * 18.0);
            app.gpu_util_history.push_back(30.0 + wave * 30.0);
            app.power_history.push_back(820.0 + wave * 250.0);
        }
        app
    }

    fn render(app: &App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut output = String::new();
        for y in 0..height {
            for x in 0..width {
                output.push_str(buffer[(x, y)].symbol());
            }
            output.push('\n');
        }
        output
    }

    #[test]
    fn wide_dashboard_contains_each_visual_group() {
        let output = render(&demo_app(), 120, 36);
        for title in [
            "PROMPT EVAL",
            "GENERATE",
            "GPU UTIL",
            "POWER",
            "SLOTS",
            "GPU MEMORY",
            "SESSION",
        ] {
            assert!(output.contains(title), "missing {title} from render");
        }
        assert!(output.contains("LAST PP"));
        assert!(output.contains('🦙'));
        assert!(output.contains("ltop"));
        assert!(!output.contains('⚡'));
        assert!(output.contains("EVAL TOK"));
        assert!(output.contains("↑ eval 70.7K"));
        assert!(output.contains("↓ out 28.8K"));
        assert!(output.contains("· 2s"));
        assert!(output.contains("-/+ 2s"));
        assert!(output.contains("42% · 58°C · 255W"));
    }

    #[test]
    fn compact_layout_uses_the_selected_section() {
        let mut app = demo_app();
        app.current_section = Section::Throughput;
        let output = render(&app, 80, 24);
        assert!(output.contains("PROMPT EVAL"));
        assert!(output.contains("GENERATE"));
        assert!(!output.contains("GPU MEMORY"));
    }

    #[test]
    fn gpu_memory_panel_leads_with_capacity_and_keeps_telemetry_secondary() {
        let mut app = demo_app();
        app.current_section = Section::Gpu;
        let output = render(&app, 80, 24);

        assert!(output.contains("81.8 / 96.0 GiB · 85%"));
        assert!(output.contains("20.0 / 24.0 GiB"));
        assert!(output.contains("RTX 4090"));
        assert!(output.contains("42% · 58°C · 255W"));
        assert!(!output.contains("VRAM "));
    }

    #[test]
    fn clipped_gpu_memory_list_exposes_its_visible_range() {
        let mut app = demo_app();
        app.current_section = Section::Gpu;
        app.snapshot.gpus = (0..8)
            .map(|index| GpuInfo {
                index,
                name: "NVIDIA GeForce RTX 4090".to_string(),
                gpu_util: 40.0 + index as f64,
                mem_used: 20_000 + index as u64 * 100,
                mem_total: 24_564,
                temp: 55.0 + index as f64,
                power_draw: 240.0 + index as f64,
                power_limit: 450.0,
                ..GpuInfo::default()
            })
            .collect();
        app.scroll = 2;

        let output = render(&app, 80, 24);

        assert!(output.contains("3–6/8"));
        assert!(output.contains("GPU 2"));
        assert!(!output.contains("GPU 1"));
    }

    #[test]
    fn gpu_memory_bar_uses_the_theme_gradient_and_a_quiet_track() {
        let app = demo_app();
        let colors = app.theme.colors();
        let spans = gradient_progress_bar(50.0, 8, &app.theme.memory, colors.track);
        let symbols: String = spans.iter().map(|span| span.content.as_ref()).collect();

        assert_eq!(symbols, "████░░░░");
        assert_ne!(spans[0].style.fg, spans[3].style.fg);
        assert_eq!(spans.last().unwrap().style.fg, Some(colors.track));
    }

    #[test]
    fn prompt_rate_scope_is_visible_instead_of_implying_a_live_rate() {
        let mut app = demo_app();
        app.current_section = Section::Throughput;
        app.prompt_rate_basis = PromptRateBasis::ServerAverage;
        let output = render(&app, 80, 24);

        assert!(output.contains("tok/s avg"));
        assert!(!output.contains("tok/s last"));
    }

    #[test]
    fn slots_section_explains_lane_activity_without_stale_idle_counters() {
        let mut app = demo_app();
        app.current_section = Section::Slots;
        let output = render(&app, 80, 24);

        for heading in ["CONTEXT", "EVAL", "CACHED", "OUTPUT"] {
            assert!(
                output.contains(heading),
                "missing {heading} from slot table"
            );
        }
        assert!(output.contains("82%"));
        assert!(!output.contains("2.4K"));
        assert!(!output.contains("321"));
    }

    #[test]
    fn slots_section_reports_endpoint_failures() {
        let mut app = demo_app();
        app.current_section = Section::Slots;
        app.snapshot.slots.clear();
        app.snapshot.slots_error = Some("cannot reach /slots: HTTP 404".to_string());
        let output = render(&app, 80, 24);

        assert!(output.contains("telemetry unavailable"));
        assert!(output.contains("cannot reach /slots: HTTP 404"));
    }

    #[test]
    fn help_is_rendered_as_a_modal() {
        let mut app = demo_app();
        app.show_help = true;
        let output = render(&app, 120, 36);
        assert!(output.contains("KEYBOARD"));
        assert!(output.contains("DATA SOURCES"));
        assert!(output.contains("- / +"));
        assert!(output.contains("One 2s cadence"));
        assert!(output.contains("quick help"));
    }

    #[test]
    fn theme_picker_exposes_live_preview_controls_and_bundled_palettes() {
        let mut app = demo_app();
        app.open_theme_picker();
        let output = render(&app, 60, 20);

        assert!(output.contains("THEME"));
        assert!(output.contains("applied"));
        assert!(output.contains("preview"));
        assert!(output.contains("Tokyo Night"));
        assert!(output.contains("bg:on"));
        assert!(output.contains("Enter"));
        assert!(output.contains("cancel"));
    }

    #[test]
    fn theme_background_is_applied_to_the_whole_frame_and_can_be_disabled() {
        let catalog = ThemeCatalog::builtin_only();
        let theme_index = catalog.index_of("Solarized Light").unwrap();
        let mut app = App::with_theme_catalog(
            "http://127.0.0.1:8080".to_string(),
            catalog,
            theme_index,
            true,
        );
        let expected = app.theme.background.unwrap();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        assert_eq!(terminal.backend().buffer()[(0, 0)].bg, expected);

        app.theme_background = false;
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        assert_eq!(terminal.backend().buffer()[(0, 0)].bg, Color::Reset);
    }

    #[test]
    fn tiny_terminals_get_a_clear_fallback() {
        let output = render(&demo_app(), 50, 15);
        assert!(output.contains("Terminal too small"));
        assert!(output.contains("60×20"));
    }
}
