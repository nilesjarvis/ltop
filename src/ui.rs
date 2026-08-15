use crate::api::GpuInfo;
use crate::app::{App, CacheRequestObservation, PromptRateBasis, Section, MAX_SAMPLES};
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
    let uptime = app
        .snapshot
        .local_server
        .as_ref()
        .and_then(|server| server.process_uptime_seconds)
        .map(|seconds| format!("up {}", fmt_duration(seconds)))
        .unwrap_or_else(|| format!("view {}", app.uptime_str()));

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
                label_span("LIFETIME  ", colors),
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
                Constraint::Percentage(32),
                Constraint::Percentage(36),
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
                label_span("SERVICE  ", colors),
                Span::styled(
                    format!(
                        "{} · {} ctx · {}",
                        fmt_slot_count(slot_count),
                        fmt_num(context as f64),
                        uptime
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
                label_span("CONFIG  ", colors),
                Span::styled(
                    format!(
                        "{} · {} ctx",
                        fmt_slot_count(slot_count),
                        fmt_num(context as f64)
                    ),
                    Style::default().fg(colors.text),
                ),
            ])),
            bottom[1],
        );
    }
}

fn draw_body(frame: &mut Frame, area: Rect, app: &App) {
    match app.current_section {
        Section::Service => draw_service_view(frame, area, app),
        Section::Cache => draw_cache_view(frame, area, app),
        _ if area.width >= DASHBOARD_WIDTH && area.height >= DASHBOARD_HEIGHT => {
            draw_dashboard(frame, area, app);
        }
        _ => draw_section(frame, area, app),
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
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Length(8),
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
            let metrics_height = if area.height <= 14 {
                Constraint::Length(8)
            } else {
                Constraint::Percentage(45)
            };
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([metrics_height, Constraint::Length(1), Constraint::Fill(1)])
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
        Section::Service => draw_service_view(frame, area, app),
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
        Section::Cache => draw_cache_view(frame, area, app),
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

fn draw_service_view(frame: &mut Frame, area: Rect, app: &App) {
    let colors = app.theme.colors();
    let block = panel_block(
        panel_title("SERVICE & WORKLOAD", colors.title),
        panel_value("full context ".to_string(), colors.dim),
        true,
        colors.border_highlight,
        colors,
    );
    let inner = block.inner(area);
    block.render(area, frame.buffer_mut());

    let identity = service_identity_lines(app, colors);
    let workload = workload_detail_lines(app, colors);
    if inner.width >= 96 {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Length(2),
                Constraint::Fill(1),
            ])
            .split(inner);
        frame.render_widget(
            Paragraph::new(identity)
                .wrap(Wrap { trim: false })
                .scroll((app.scroll, 0)),
            columns[0],
        );
        frame.render_widget(
            Paragraph::new(workload)
                .wrap(Wrap { trim: false })
                .scroll((app.scroll, 0)),
            columns[2],
        );
    } else {
        let mut lines = identity;
        lines.push(Line::from(""));
        lines.extend(workload);
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((app.scroll, 0)),
            inner,
        );
    }
}

fn draw_cache_view(frame: &mut Frame, area: Rect, app: &App) {
    let colors = app.theme.colors();
    let subtitle = if area.width >= 78 {
        "prompt reuse · context · configuration "
    } else {
        "reuse · context "
    };
    let block = panel_block(
        panel_title("CACHE", colors.title),
        panel_value(subtitle.to_string(), colors.dim),
        true,
        colors.border_highlight,
        colors,
    );
    let inner = block.inner(area);
    block.render(area, frame.buffer_mut());

    let observed = cache_observed_lines(app, colors);
    let configuration = cache_configuration_lines(app, colors);
    let slots = cache_slot_lines(app, colors);

    if inner.width >= 96 {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(55),
                Constraint::Length(2),
                Constraint::Fill(1),
            ])
            .split(inner);
        let mut left = cache_summary_lines(app, colors, columns[0].width);
        left.push(Line::from(""));
        left.extend(observed);
        left.push(Line::from(""));
        left.extend(configuration);
        frame.render_widget(
            Paragraph::new(left)
                .wrap(Wrap { trim: false })
                .scroll((app.scroll, 0)),
            columns[0],
        );
        frame.render_widget(
            Paragraph::new(slots)
                .wrap(Wrap { trim: false })
                .scroll((app.scroll, 0)),
            columns[2],
        );
    } else {
        let mut lines = cache_summary_lines(app, colors, inner.width);
        lines.push(Line::from(""));
        lines.extend(observed);
        lines.push(Line::from(""));
        lines.extend(slots);
        lines.push(Line::from(""));
        lines.extend(configuration);
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((app.scroll, 0)),
            inner,
        );
    }
}

fn cache_summary_lines(app: &App, colors: ThemeColors, width: u16) -> Vec<Line<'static>> {
    let mut active: Vec<&CacheRequestObservation> = app.active_cache_requests().collect();
    active.sort_by_key(|request| request.slot_id);
    let source_error = app.snapshot.slots_error.as_deref();

    if active.is_empty() {
        if let Some(last) = app.last_cache_request() {
            let task = last
                .task_id
                .map_or_else(|| "task —".to_string(), |id| format!("task {id}"));
            let state = format!(
                "{task} · slot {} · {} · {}",
                last.slot_id,
                last.phase,
                fmt_cache_age(last.last_seen.elapsed().as_secs())
            );
            let mut lines = vec![detail_heading("PROMPT REUSE — LAST OBSERVED", colors)];
            lines.extend(cache_request_lines(
                state,
                last.reused_tokens,
                last.evaluated_tokens,
                last.context_tokens,
                last.context_capacity,
                last.output_tokens,
                last.provisional(),
                width,
                app,
                colors,
            ));
            if let Some(error) = source_error {
                lines.push(detail_line(
                    "SOURCE",
                    error.to_string(),
                    colors.error,
                    colors,
                ));
            }
            return lines;
        }

        return vec![
            detail_heading("PROMPT REUSE", colors),
            detail_line(
                "STATE",
                source_error
                    .unwrap_or("no cache-bearing request observed yet")
                    .to_string(),
                if source_error.is_some() {
                    colors.error
                } else {
                    colors.dim
                },
                colors,
            ),
            cache_reuse_bar_line(None, width, app, colors),
            detail_line(
                "INPUT",
                "waiting for /slots prompt counters".to_string(),
                colors.dim,
                colors,
            ),
        ];
    }

    let reused = active.iter().fold(0_u64, |total, request| {
        total.saturating_add(request.reused_tokens)
    });
    let evaluated = active.iter().fold(0_u64, |total, request| {
        total.saturating_add(request.evaluated_tokens)
    });
    let context = active.iter().fold(0_u64, |total, request| {
        total.saturating_add(request.context_tokens)
    });
    let capacity = active.iter().fold(0_u64, |total, request| {
        total.saturating_add(request.context_capacity)
    });
    let output = active.iter().fold(0_u64, |total, request| {
        total.saturating_add(request.output_tokens)
    });
    let provisional = active.iter().any(|request| request.provisional());
    let phase = if active.iter().all(|request| request.phase == "prefill") {
        "prefill"
    } else if active.iter().all(|request| request.phase == "decode") {
        "decode"
    } else {
        "mixed phases"
    };
    let state = if active.len() == 1 {
        let request = active[0];
        let task = request
            .task_id
            .map_or_else(|| "task —".to_string(), |id| format!("task {id}"));
        format!("{task} · slot {} · {phase}", request.slot_id)
    } else {
        format!("{} active slots · {phase}", active.len())
    };
    let state = if source_error.is_some() {
        format!("{state} · telemetry stale")
    } else {
        state
    };

    let mut lines = vec![detail_heading(
        if source_error.is_some() {
            "PROMPT REUSE — LAST SAMPLE"
        } else {
            "PROMPT REUSE — CURRENT"
        },
        colors,
    )];
    lines.extend(cache_request_lines(
        state,
        reused,
        evaluated,
        context,
        capacity,
        output,
        provisional,
        width,
        app,
        colors,
    ));
    if let Some(error) = source_error {
        lines.push(detail_line(
            "SOURCE",
            error.to_string(),
            colors.error,
            colors,
        ));
    }
    lines
}

#[allow(clippy::too_many_arguments)]
fn cache_request_lines(
    state: String,
    reused: u64,
    evaluated: u64,
    context: u64,
    capacity: u64,
    output: u64,
    provisional: bool,
    width: u16,
    app: &App,
    colors: ThemeColors,
) -> Vec<Line<'static>> {
    let input = reused.saturating_add(evaluated);
    let reuse_percent = (input > 0).then(|| reused as f64 / input as f64 * 100.0);
    let state = format!(
        "{state} · {}",
        if provisional {
            "provisional"
        } else {
            "settled"
        }
    );
    let context_percent = (capacity > 0).then(|| context as f64 / capacity as f64 * 100.0);
    let headroom_value = if capacity > 0 {
        format!(
            "{} tokens · {} / {} occupied",
            fmt_num(capacity.saturating_sub(context) as f64),
            fmt_num(context as f64),
            fmt_num(capacity as f64)
        )
    } else {
        format!("{} / — · capacity unavailable", fmt_num(context as f64))
    };
    let input_value = if input > 0 {
        format!(
            "{} = {} reused + {} evaluated",
            fmt_num(input as f64),
            fmt_num(reused as f64),
            fmt_num(evaluated as f64)
        )
    } else {
        "prompt counters have not advanced yet".to_string()
    };

    vec![
        detail_line("REQUEST", state, colors.text, colors),
        cache_reuse_bar_line(reuse_percent, width, app, colors),
        detail_line("INPUT", input_value, colors.text, colors),
        cache_context_bar_line(context_percent, width, app, colors),
        detail_line(
            "HEADROOM",
            headroom_value,
            app.theme.memory.at(context_percent.unwrap_or(0.0)),
            colors,
        ),
        detail_line(
            "CONTENT",
            format!(
                "{} input + {} output currently represented",
                fmt_num(input as f64),
                fmt_num(output as f64)
            ),
            colors.dim,
            colors,
        ),
    ]
}

fn cache_reuse_bar_line(
    percent: Option<f64>,
    width: u16,
    app: &App,
    colors: ThemeColors,
) -> Line<'static> {
    let suffix = percent.map_or_else(|| "   —".to_string(), |value| format!(" {value:>3.0}%"));
    let bar_width = width
        .saturating_sub(13 + suffix.chars().count() as u16 + 2)
        .max(4) as usize;
    let mut spans = vec![
        Span::styled(
            format!("{:<13}", "REUSE"),
            Style::default().fg(colors.dim).add_modifier(Modifier::BOLD),
        ),
        Span::styled("[", Style::default().fg(colors.dim)),
    ];
    if let Some(percent) = percent {
        let percent = percent.clamp(0.0, 100.0);
        let reused_width = (percent / 100.0 * bar_width as f64).round() as usize;
        let evaluated_width = bar_width.saturating_sub(reused_width);
        if reused_width > 0 {
            spans.push(Span::styled(
                "█".repeat(reused_width),
                Style::default().fg(app.theme.cache.at(percent)),
            ));
        }
        if evaluated_width > 0 {
            spans.push(Span::styled(
                "▒".repeat(evaluated_width),
                Style::default().fg(colors.prompt),
            ));
        }
    } else {
        spans.push(Span::styled(
            "░".repeat(bar_width),
            Style::default().fg(colors.track),
        ));
    }
    spans.push(Span::styled("]", Style::default().fg(colors.dim)));
    spans.push(Span::styled(suffix, value_style(colors)));
    Line::from(spans)
}

fn cache_context_bar_line(
    percent: Option<f64>,
    width: u16,
    app: &App,
    colors: ThemeColors,
) -> Line<'static> {
    let suffix = percent.map_or_else(|| "   —".to_string(), |value| format!(" {value:>3.0}%"));
    let bar_width = width
        .saturating_sub(13 + suffix.chars().count() as u16 + 2)
        .max(4) as usize;
    let mut spans = vec![
        Span::styled(
            format!("{:<13}", "CONTEXT"),
            Style::default().fg(colors.dim).add_modifier(Modifier::BOLD),
        ),
        Span::styled("[", Style::default().fg(colors.dim)),
    ];
    spans.extend(gradient_progress_bar(
        percent.unwrap_or(0.0),
        bar_width,
        &app.theme.memory,
        colors.track,
    ));
    spans.push(Span::styled("]", Style::default().fg(colors.dim)));
    spans.push(Span::styled(suffix, value_style(colors)));
    Line::from(spans)
}

fn cache_observed_lines(app: &App, colors: ThemeColors) -> Vec<Line<'static>> {
    let totals = app.cache_observed_totals();
    let input = totals.input_tokens();
    let mut lines = vec![detail_heading("OBSERVED SINCE LTOP START", colors)];
    lines.push(detail_line(
        "WINDOW",
        format!("{} view uptime · /slots samples", app.uptime_str()),
        colors.text,
        colors,
    ));
    lines.push(detail_line(
        "REQUESTS",
        format!("{} observed", totals.requests),
        colors.text,
        colors,
    ));
    if input > 0 {
        lines.push(detail_line(
            "REUSE",
            format!(
                "{:.1}% weighted · {} tokens",
                totals.reuse_percent(),
                fmt_num(totals.reused_tokens as f64)
            ),
            app.theme.cache.at(totals.reuse_percent()),
            colors,
        ));
        lines.push(detail_line(
            "INPUT",
            format!(
                "{} total · {} evaluated",
                fmt_num(input as f64),
                fmt_num(totals.evaluated_tokens as f64)
            ),
            colors.prompt,
            colors,
        ));
    } else {
        lines.push(detail_line(
            "TOKENS",
            "no prompt tokens observed yet".to_string(),
            colors.dim,
            colors,
        ));
    }
    lines.push(detail_line(
        "SCOPE",
        "active + completed tasks seen while ltop is open".to_string(),
        colors.dim,
        colors,
    ));
    lines.push(detail_line(
        "LIMIT",
        "short requests between polls can be missed".to_string(),
        colors.dim,
        colors,
    ));
    lines
}

fn cache_configuration_lines(app: &App, colors: ThemeColors) -> Vec<Line<'static>> {
    let mut lines = vec![detail_heading("CACHE CONFIGURATION", colors)];
    if let Some(server) = app.snapshot.local_server.as_ref() {
        let key_type = if server.cache_type_k.is_empty() {
            "—"
        } else {
            server.cache_type_k.as_str()
        };
        let value_type = if server.cache_type_v.is_empty() {
            "—"
        } else {
            server.cache_type_v.as_str()
        };
        lines.push(detail_line(
            "KV TYPE",
            format!("K {key_type} / V {value_type}"),
            colors.text,
            colors,
        ));
        lines.push(detail_line(
            "RAM BUDGET",
            server.cache_ram_mib.map_or_else(
                || "not configured or unavailable".to_string(),
                |mib| {
                    format!(
                        "{} configured · current use not exposed",
                        fmt_memory(mib.max(0) as u64)
                    )
                },
            ),
            colors.text,
            colors,
        ));
    } else {
        lines.push(detail_line(
            "RUNTIME",
            "launch cache settings unavailable for this server".to_string(),
            colors.dim,
            colors,
        ));
    }
    lines.push(detail_line(
        "MEANING",
        "reuse = reused ÷ (reused + evaluated)".to_string(),
        colors.dim,
        colors,
    ));
    lines.push(detail_line(
        "NOT EXPOSED",
        "actual KV bytes, entries, evictions, or time saved".to_string(),
        colors.dim,
        colors,
    ));
    lines
}

fn cache_slot_lines(app: &App, colors: ThemeColors) -> Vec<Line<'static>> {
    let mut lines = vec![detail_heading("SLOT CACHE", colors), Line::from("")];
    if app.snapshot.slots.is_empty() {
        let message = app
            .snapshot
            .slots_error
            .as_deref()
            .unwrap_or(if app.snapshot.connected {
                "No slots reported"
            } else {
                "Server unreachable"
            });
        lines.push(detail_line(
            "STATE",
            message.to_string(),
            if app.snapshot.slots_error.is_some() {
                colors.error
            } else {
                colors.dim
            },
            colors,
        ));
        return lines;
    }

    lines.push(Line::from(Span::styled(
        format!(
            "{:<3}{:<9}{:>8}{:>8}{:>8}{:>8}",
            "ID", "PHASE", "INPUT", "REUSE", "EVAL", "CTX"
        ),
        Style::default().fg(colors.dim).add_modifier(Modifier::BOLD),
    )));
    for slot in &app.snapshot.slots {
        let phase = slot.phase();
        let context_percent = if slot.context_capacity > 0 {
            slot.context_tokens.max(0) as f64 / slot.context_capacity as f64 * 100.0
        } else {
            0.0
        };
        let context = if slot.context_capacity > 0 {
            format!("{context_percent:.0}%")
        } else {
            "—".to_string()
        };
        let (input, reuse, evaluated) = if slot.is_processing {
            let reused = slot.prompt_tokens_cached.max(0) as u64;
            let evaluated = slot.prompt_tokens_processed.max(0) as u64;
            let input = reused.saturating_add(evaluated);
            let reuse = if input > 0 {
                format!("{:.0}%", reused as f64 / input as f64 * 100.0)
            } else {
                "—".to_string()
            };
            (fmt_num(input as f64), reuse, fmt_num(evaluated as f64))
        } else {
            ("—".to_string(), "—".to_string(), "—".to_string())
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<3}", slot.id),
                Style::default()
                    .fg(colors.bright)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{phase:<9}"),
                Style::default().fg(if slot.is_processing {
                    colors.ok
                } else {
                    colors.dim
                }),
            ),
            Span::styled(format!("{input:>8}"), Style::default().fg(colors.text)),
            Span::styled(
                format!("{reuse:>8}"),
                Style::default().fg(if slot.is_processing {
                    app.theme.cache.at(slot.prompt_tokens_cached.max(0) as f64
                        / (slot.prompt_tokens_cached.max(0) + slot.prompt_tokens_processed.max(0))
                            .max(1) as f64
                        * 100.0)
                } else {
                    colors.dim
                }),
            ),
            Span::styled(
                format!("{evaluated:>8}"),
                Style::default().fg(if slot.is_processing {
                    colors.prompt
                } else {
                    colors.dim
                }),
            ),
            Span::styled(
                format!("{context:>8}"),
                Style::default().fg(app.theme.memory.at(context_percent)),
            ),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(detail_heading("HOW TO READ IT", colors));
    lines.push(detail_line(
        "REUSED",
        "input tokens served without prompt evaluation".to_string(),
        app.theme.cache.at(75.0),
        colors,
    ));
    lines.push(detail_line(
        "EVALUATED",
        "input tokens the model processed during prefill".to_string(),
        colors.prompt,
        colors,
    ));
    lines.push(detail_line(
        "HEADROOM",
        "remaining token capacity, not free KV bytes".to_string(),
        colors.dim,
        colors,
    ));
    lines.push(detail_line(
        "LOW REUSE",
        "normal for a new or unrelated prompt".to_string(),
        colors.dim,
        colors,
    ));
    lines
}

fn fmt_cache_age(seconds: u64) -> String {
    if seconds < 2 {
        "just now".to_string()
    } else {
        format!("{} ago", fmt_duration(seconds))
    }
}

fn speculative_types_enabled(value: &str) -> bool {
    value
        .split(',')
        .map(str::trim)
        .any(|kind| !kind.is_empty() && !kind.eq_ignore_ascii_case("none"))
}

fn service_identity_lines(app: &App, colors: ThemeColors) -> Vec<Line<'static>> {
    let props = app.snapshot.props.as_ref();
    let model = app.snapshot.model.as_ref();
    let local = app.snapshot.local_server.as_ref();
    let host = app.snapshot.host.as_ref();
    let model_name = model
        .map(|model| model.id.as_str())
        .filter(|name| !name.is_empty())
        .or_else(|| props.map(|props| props.model_alias.as_str()))
        .filter(|name| !name.is_empty())
        .unwrap_or("unavailable");
    let target = props
        .map(|props| props.model_path.as_str())
        .filter(|path| !path.is_empty())
        .map(path_basename)
        .unwrap_or_else(|| "not reported".to_string());
    let ftype = model
        .map(|model| model.ftype.as_str())
        .filter(|value| !value.is_empty())
        .or_else(|| props.map(|props| props.model_ftype.as_str()))
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown");
    let parameter_count = model
        .filter(|model| model.parameter_count > 0)
        .map(|model| fmt_num(model.parameter_count as f64))
        .unwrap_or_else(|| "—".to_string());
    let model_size = model
        .filter(|model| model.size_bytes > 0)
        .map(|model| fmt_bytes(model.size_bytes))
        .unwrap_or_else(|| "—".to_string());
    let architecture = model.map_or_else(
        || "unavailable".to_string(),
        |model| {
            format!(
                "{} vocab · {} embedding",
                fmt_num(model.vocabulary_size.max(0) as f64),
                fmt_num(model.embedding_size.max(0) as f64)
            )
        },
    );
    let format = model
        .map(|model| model.format.as_str())
        .filter(|format| !format.is_empty())
        .unwrap_or("format unknown");
    let active_context = props
        .map(|props| props.n_ctx)
        .filter(|context| *context > 0)
        .or_else(|| {
            model
                .map(|model| model.context_size)
                .filter(|context| *context > 0)
        });
    let trained_context = model
        .map(|model| model.trained_context_size)
        .filter(|context| *context > 0);
    let context = match (active_context, trained_context) {
        (Some(active), Some(trained)) => {
            format!(
                "{} active · {} trained",
                fmt_num(active as f64),
                fmt_num(trained as f64)
            )
        }
        (Some(active), None) => format!("{} active", fmt_num(active as f64)),
        _ => "unavailable".to_string(),
    };
    let build = props
        .map(|props| props.build_info.as_str())
        .filter(|build| !build.is_empty())
        .unwrap_or("unavailable");
    let draft = local
        .map(|server| server.draft_model.as_str())
        .filter(|draft| !draft.is_empty())
        .unwrap_or("not reported");
    let slot_speculative =
        app.snapshot.slots.iter().any(|slot| {
            slot.speculative || speculative_types_enabled(&slot.params.speculative_types)
        });
    let speculative = local
        .map(|server| {
            let typed_speculation = speculative_types_enabled(&server.speculative_type);
            let configured = !server.draft_model.is_empty()
                || typed_speculation
                || server.speculative_max_tokens.is_some()
                || slot_speculative;
            if !configured {
                return "off".to_string();
            }
            let kind = if typed_speculation {
                server.speculative_type.clone()
            } else if slot_speculative {
                "enabled".to_string()
            } else {
                "configured".to_string()
            };
            server
                .speculative_max_tokens
                .map_or(kind.clone(), |maximum| format!("{kind} · max {maximum}"))
        })
        .unwrap_or_else(|| {
            if slot_speculative {
                "enabled · limits not reported".to_string()
            } else {
                "off".to_string()
            }
        });
    let engine = local.map_or_else(
        || "runtime flags unavailable for remote server".to_string(),
        |server| {
            let devices = if server.devices.is_empty() {
                "devices —".to_string()
            } else {
                format!("{} devices", server.devices.split(',').count())
            };
            let split = if server.split_mode.is_empty() {
                "split —".to_string()
            } else {
                format!("{} split", server.split_mode)
            };
            let parallel = server
                .parallel
                .map(|parallel| format!("parallel {parallel}"))
                .unwrap_or_else(|| "parallel —".to_string());
            let batches = match (server.batch_size, server.ubatch_size) {
                (Some(batch), Some(ubatch)) => format!("batch {batch}/{ubatch}"),
                _ => "batch —".to_string(),
            };
            let flash = server
                .flash_attention
                .map(|enabled| {
                    if enabled {
                        "flash-attn"
                    } else {
                        "no flash-attn"
                    }
                })
                .unwrap_or("flash-attn —");
            format!("{devices} · {split} · {parallel} · {batches} · {flash}")
        },
    );
    let cache = local.map_or_else(
        || "runtime cache configuration unavailable".to_string(),
        |server| {
            let types = match (server.cache_type_k.as_str(), server.cache_type_v.as_str()) {
                ("", "") => "KV type —".to_string(),
                (key, value) => format!("KV {key}/{value}"),
            };
            let ram = server
                .cache_ram_mib
                .map(|mib| format!("RAM cache {}", fmt_memory(mib.max(0) as u64)))
                .unwrap_or_else(|| "RAM cache —".to_string());
            format!("{types} · {ram}")
        },
    );
    let defaults = props.map_or_else(
        || "unavailable".to_string(),
        |props| {
            let params = &props.default_generation;
            format!(
                "temp {} · top-k {} · top-p {} · min-p {}",
                fmt_optional_f64(params.temperature),
                params
                    .top_k
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "—".to_string()),
                fmt_optional_f64(params.top_p),
                fmt_optional_f64(params.min_p)
            )
        },
    );
    let capabilities = props.map_or_else(
        || "unavailable".to_string(),
        |props| {
            let mut values = vec!["text"];
            if props.modalities.vision {
                values.push("vision");
            }
            if props.modalities.video {
                values.push("video");
            }
            if props.modalities.audio {
                values.push("audio");
            }
            if props.chat_capabilities.tools {
                values.push("tools");
            }
            if props.chat_capabilities.parallel_tool_calls {
                values.push("parallel tools");
            }
            values.join(" · ")
        },
    );
    let endpoints = props.map_or_else(
        || "unavailable".to_string(),
        |props| {
            format!(
                "metrics {} · slots {} · web UI {} · CORS proxy {}",
                on_off(props.endpoint_metrics),
                on_off(props.endpoint_slots),
                on_off(props.ui_enabled),
                on_off(props.cors_proxy_enabled)
            )
        },
    );
    let listen = local.map_or_else(
        || format!("{} · remote process details unavailable", app.url),
        |server| {
            format!(
                "{}:{} · API key {}",
                server.bind_host,
                server.port,
                if server.api_key_configured {
                    "set"
                } else {
                    "not set"
                }
            )
        },
    );
    let listen_color = if local.is_some_and(|server| {
        matches!(server.bind_host.as_str(), "0.0.0.0" | "::") && !server.api_key_configured
    }) {
        colors.warn
    } else {
        colors.text
    };
    let runtime = local.map_or_else(
        || "remote or unmatched process".to_string(),
        |server| {
            let uptime = server
                .process_uptime_seconds
                .map(fmt_duration)
                .unwrap_or_else(|| "—".to_string());
            let rss = server
                .rss_kib
                .map(fmt_kib)
                .unwrap_or_else(|| "—".to_string());
            let threads = server
                .threads
                .map(|threads| format!("{threads} threads"))
                .unwrap_or_else(|| "threads —".to_string());
            format!("PID {} · up {uptime} · RSS {rss} · {threads}", server.pid)
        },
    );
    let allocation = local.map_or_else(
        || "unavailable".to_string(),
        |server| {
            let current = server
                .cgroup_memory_current
                .map(fmt_bytes)
                .unwrap_or_else(|| "—".to_string());
            let limit = server
                .cgroup_memory_limit
                .map(fmt_bytes)
                .unwrap_or_else(|| "unlimited".to_string());
            let swap = server
                .cgroup_swap_limit
                .map(fmt_bytes)
                .unwrap_or_else(|| "unlimited".to_string());
            format!("{current} / {limit} · swap {swap}")
        },
    );
    let host_summary = host.map_or_else(
        || "unavailable for remote server".to_string(),
        |host| {
            let used = host
                .memory_total_kib
                .saturating_sub(host.memory_available_kib);
            let swap_used = host.swap_total_kib.saturating_sub(host.swap_free_kib);
            format!(
                "RAM {} / {} · swap {} / {} · load {:.2}/{:.2}/{:.2} · {} CPU",
                fmt_kib(used),
                fmt_kib(host.memory_total_kib),
                fmt_kib(swap_used),
                fmt_kib(host.swap_total_kib),
                host.load_one,
                host.load_five,
                host.load_fifteen,
                host.logical_cpus
            )
        },
    );
    let gpu = if app.snapshot.gpus.is_empty() {
        app.snapshot
            .gpu_error
            .clone()
            .unwrap_or_else(|| "unavailable".to_string())
    } else {
        let (used, total) = gpu_memory_totals(&app.snapshot.gpus);
        let core_clock_min = app
            .snapshot
            .gpus
            .iter()
            .map(|gpu| gpu.clock_gr)
            .filter(|clock| *clock > 0)
            .min();
        let core_clock_max = app
            .snapshot
            .gpus
            .iter()
            .map(|gpu| gpu.clock_gr)
            .filter(|clock| *clock > 0)
            .max();
        let fan_min = app
            .snapshot
            .gpus
            .iter()
            .map(|gpu| gpu.fan_speed)
            .filter(|fan| *fan > 0.0)
            .reduce(f64::min);
        let fan_max = app
            .snapshot
            .gpus
            .iter()
            .map(|gpu| gpu.fan_speed)
            .filter(|fan| *fan > 0.0)
            .reduce(f64::max);
        let clocks = match (core_clock_min, core_clock_max) {
            (Some(minimum), Some(maximum)) if minimum == maximum => format!(" · {minimum} MHz"),
            (Some(minimum), Some(maximum)) => format!(" · {minimum}–{maximum} MHz"),
            _ => String::new(),
        };
        let fans = match (fan_min, fan_max) {
            (Some(minimum), Some(maximum)) if (minimum - maximum).abs() < f64::EPSILON => {
                format!(" · fan {minimum:.0}%")
            }
            (Some(minimum), Some(maximum)) => format!(" · fan {minimum:.0}–{maximum:.0}%"),
            _ => String::new(),
        };
        format!(
            "{} devices · {} / {} VRAM · {:.0}% avg util{clocks}{fans}",
            app.snapshot.gpus.len(),
            fmt_memory(used),
            fmt_memory(total),
            average_gpu_util(app)
        )
    };
    let source_health = source_health_summary(app);
    let source_color = if app.snapshot.error.is_some() {
        colors.warn
    } else {
        colors.ok
    };

    let mut lines = vec![
        detail_heading("IDENTITY", colors),
        detail_line("MODEL", model_name.to_string(), colors.bright, colors),
        detail_line("TARGET", target, colors.text, colors),
        detail_line(
            "SCALE",
            format!("{parameter_count} params · {model_size} · {format} · {ftype}"),
            colors.text,
            colors,
        ),
        detail_line("ARCH", architecture, colors.text, colors),
        detail_line("CONTEXT", context, colors.text, colors),
        detail_line("BUILD", build.to_string(), colors.text, colors),
        detail_line("DRAFT", draft.to_string(), colors.text, colors),
        detail_line("SPEC", speculative, colors.text, colors),
        detail_line("ENGINE", engine, colors.text, colors),
        detail_line("CACHE", cache, colors.text, colors),
        detail_line("DEFAULTS", defaults, colors.text, colors),
        detail_line("CAPS", capabilities, colors.text, colors),
        detail_line("ENDPOINTS", endpoints, colors.text, colors),
        detail_line("LISTEN", listen, listen_color, colors),
        Line::from(""),
        detail_heading("PROCESS & HOST", colors),
        detail_line("PROCESS", runtime, colors.text, colors),
        detail_line("CGROUP RAM", allocation, colors.text, colors),
        detail_line("HOST", host_summary, colors.text, colors),
        detail_line("GPU", gpu, colors.text, colors),
        Line::from(""),
        detail_heading("SOURCE HEALTH", colors),
        detail_line("SOURCES", source_health, source_color, colors),
    ];
    if let Some(error) = detailed_source_error(app) {
        lines.push(detail_line("DETAIL", error, colors.warn, colors));
    }
    lines
}

fn workload_detail_lines(app: &App, colors: ThemeColors) -> Vec<Line<'static>> {
    let metrics = &app.snapshot.metrics;
    let props = app.snapshot.props.as_ref();
    let slots = props
        .map(|props| props.total_slots.max(0) as usize)
        .unwrap_or(app.snapshot.slots.len());
    let active = app
        .snapshot
        .slots
        .iter()
        .filter(|slot| slot.is_processing)
        .count();
    let prompt_scope = match app.prompt_rate_basis {
        PromptRateBasis::Interval => "last",
        PromptRateBasis::ServerAverage => "lifetime",
        PromptRateBasis::Unavailable => "unavailable",
    };
    let rates = format!(
        "PP {:.1} tok/s {prompt_scope} · TG {:.1} live / {:.1} lifetime",
        app.prompt_rate, app.predict_rate, metrics.predicted_tokens_seconds
    );
    let processing_time = format!(
        "PP {} · TG {} active time",
        fmt_seconds(metrics.prompt_seconds_total),
        fmt_seconds(metrics.tokens_predicted_seconds_total)
    );
    let pressure_color = if metrics.requests_deferred > 0.0 {
        colors.warn
    } else {
        colors.text
    };
    let mut lines = vec![
        detail_heading("WORKLOAD", colors),
        detail_line(
            "PRESSURE",
            format!(
                "{active}/{slots} active · {:.0} queued",
                metrics.requests_deferred
            ),
            pressure_color,
            colors,
        ),
        detail_line("RATES", rates, colors.text, colors),
        detail_line(
            "TOKENS",
            format!(
                "{} evaluated · {} generated",
                fmt_num(metrics.prompt_tokens_total),
                fmt_num(metrics.tokens_predicted_total)
            ),
            colors.text,
            colors,
        ),
        detail_line("TIME", processing_time, colors.text, colors),
        detail_line(
            "DECODES",
            format!(
                "{} calls · {:.2} tok/decode · {:.1} busy/decode",
                fmt_num(metrics.n_decode_total),
                if metrics.n_decode_total > 0.0 {
                    metrics.tokens_predicted_total / metrics.n_decode_total
                } else {
                    0.0
                },
                metrics.n_busy_slots_per_decode
            ),
            colors.text,
            colors,
        ),
        detail_line(
            "HIGH WATER",
            format!(
                "{} tokens in one decode batch",
                fmt_num(metrics.n_tokens_max)
            ),
            colors.text,
            colors,
        ),
    ];
    if metrics.requests_deferred > 0.0 {
        lines.push(detail_line(
            "QUEUE DETAIL",
            "age and client are not exposed by llama.cpp telemetry".to_string(),
            colors.dim,
            colors,
        ));
    }

    lines.push(Line::from(""));
    lines.push(detail_heading("ACTIVE REQUEST", colors));
    if let Some(slot) = app.snapshot.slots.iter().find(|slot| slot.is_processing) {
        let task = slot
            .task_id
            .map(|task| task.to_string())
            .unwrap_or_else(|| "—".to_string());
        let streaming = slot
            .params
            .stream
            .map(|stream| if stream { "streaming" } else { "buffered" })
            .unwrap_or("stream unknown");
        let input = slot.prompt_tokens_processed.max(0) + slot.prompt_tokens_cached.max(0);
        let cached_percent = if input > 0 {
            slot.prompt_tokens_cached.max(0) as f64 / input as f64 * 100.0
        } else {
            0.0
        };
        let output = slot.current_output_tokens();
        let output_limit = slot
            .params
            .max_tokens
            .map(|limit| format!(" / {} max", fmt_num(limit as f64)))
            .unwrap_or_default();
        let remaining = slot
            .remaining_tokens
            .map(|remaining| format!(" · {} remain", fmt_num(remaining as f64)))
            .unwrap_or_default();
        let mode = [
            (!slot.params.chat_format.is_empty()).then_some(slot.params.chat_format.as_str()),
            (!slot.params.reasoning_format.is_empty())
                .then_some(slot.params.reasoning_format.as_str()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        let sampling = format!(
            "temp {} · top-k {} · top-p {} · min-p {}",
            fmt_optional_f64(slot.params.temperature),
            slot.params
                .top_k
                .map(|value| value.to_string())
                .unwrap_or_else(|| "—".to_string()),
            fmt_optional_f64(slot.params.top_p),
            fmt_optional_f64(slot.params.min_p)
        );
        let spec = if slot.params.speculative_types.is_empty() {
            if slot.speculative { "enabled" } else { "off" }.to_string()
        } else {
            slot.params.speculative_types.clone()
        };
        let spec_result = app
            .snapshot
            .last_request_timings
            .as_ref()
            .and_then(|timings| {
                (timings.draft_n > 0).then(|| {
                    format!(
                        "{:.1}% accepted ({}/{})",
                        timings.draft_n_accepted as f64 / timings.draft_n as f64 * 100.0,
                        timings.draft_n_accepted,
                        timings.draft_n
                    )
                })
            });
        lines.extend([
            detail_line(
                "TASK",
                format!("{task} · {} · {streaming}", slot.phase()),
                colors.bright,
                colors,
            ),
            detail_line(
                "INPUT",
                format!(
                    "{} total · {} evaluated · {} cached ({cached_percent:.0}%)",
                    fmt_num(input as f64),
                    fmt_num(slot.prompt_tokens_processed.max(0) as f64),
                    fmt_num(slot.prompt_tokens_cached.max(0) as f64)
                ),
                colors.prompt,
                colors,
            ),
            detail_line(
                "OUTPUT",
                format!("{}{output_limit}{remaining}", fmt_num(output as f64)),
                colors.predict,
                colors,
            ),
            detail_line(
                "CONTEXT",
                format!(
                    "{} / {} ({:.1}%)",
                    fmt_num(slot.context_tokens.max(0) as f64),
                    fmt_num(slot.context_capacity.max(0) as f64),
                    if slot.context_capacity > 0 {
                        slot.context_tokens.max(0) as f64 / slot.context_capacity as f64 * 100.0
                    } else {
                        0.0
                    }
                ),
                colors.text,
                colors,
            ),
            detail_line(
                "MODE",
                if mode.is_empty() {
                    "unreported".to_string()
                } else {
                    mode
                },
                colors.text,
                colors,
            ),
            detail_line("SAMPLING", sampling, colors.text, colors),
            detail_line("SPEC MODE", spec, colors.text, colors),
            detail_line(
                "SPEC RESULT",
                spec_result.unwrap_or_else(|| {
                    "acceptance is not exposed by the polled endpoints".to_string()
                }),
                colors.dim,
                colors,
            ),
        ]);
    } else {
        lines.push(detail_line(
            "STATE",
            "no request is currently processing".to_string(),
            colors.dim,
            colors,
        ));
    }
    lines
}

fn detail_heading(title: &str, colors: ThemeColors) -> Line<'static> {
    Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(colors.accent)
            .add_modifier(Modifier::BOLD),
    ))
}

fn detail_line(
    label: &'static str,
    value: String,
    value_color: Color,
    colors: ThemeColors,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label:<13}"),
            Style::default().fg(colors.dim).add_modifier(Modifier::BOLD),
        ),
        Span::styled(value, Style::default().fg(value_color)),
    ])
}

fn on_off(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "on",
        Some(false) => "off",
        None => "—",
    }
}

fn source_health_summary(app: &App) -> String {
    let status = |available: bool, error: bool| {
        if available && error {
            "stale"
        } else if available {
            "ok"
        } else if error {
            "error"
        } else {
            "waiting"
        }
    };
    format!(
        "metrics {} · slots {} · props {} · model {} · GPU {}",
        status(
            app.snapshot.metrics_available,
            app.snapshot.metrics_error.is_some()
        ),
        status(
            !app.snapshot.slots.is_empty(),
            app.snapshot.slots_error.is_some()
        ),
        status(
            app.snapshot.props.is_some(),
            app.snapshot.props_error.is_some()
        ),
        status(
            app.snapshot.model.is_some(),
            app.snapshot.model_error.is_some()
        ),
        status(
            !app.snapshot.gpus.is_empty(),
            app.snapshot.gpu_error.is_some()
        )
    )
}

fn detailed_source_error(app: &App) -> Option<String> {
    [
        app.snapshot.metrics_error.as_deref(),
        app.snapshot.slots_error.as_deref(),
        app.snapshot.props_error.as_deref(),
        app.snapshot.model_error.as_deref(),
        app.snapshot.gpu_error.as_deref(),
    ]
    .into_iter()
    .flatten()
    .next()
    .map(str::to_string)
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
        .history_capacity(MAX_SAMPLES)
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
        BrailleChart::new(&data, 100.0, &app.theme.gpu, app.theme.graph_text)
            .history_capacity(MAX_SAMPLES)
            .block(block),
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
        BrailleChart::new(&data, max, &app.theme.power, app.theme.graph_text)
            .history_capacity(MAX_SAMPLES)
            .block(block),
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
        "SLOTS · SPEC ENABLED"
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
                Line::from(Span::styled(
                    format!("● {}", slot.phase()),
                    Style::default().fg(colors.ok),
                ))
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
                Line::from("PHASE"),
                Line::from("CONTEXT").right_aligned(),
                Line::from("EVAL").right_aligned(),
                Line::from("REUSE").right_aligned(),
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
                Line::from("PHASE"),
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
        let memory_value = if window.lines_per_gpu > 1 && inner.width >= 64 {
            full_memory_pair(gpu.mem_used, gpu.mem_total)
        } else {
            compact_memory_pair(gpu.mem_used, gpu.mem_total)
        };
        let percent = memory_percent_label(gpu.mem_total, memory_pct);
        let desired_meter_width = match inner.width {
            64.. => 10,
            48..=63 => 8,
            36..=47 => 6,
            28..=35 => 4,
            _ => 0,
        };
        let base_capacity_width = memory_value.chars().count() + 2 + percent.chars().count();
        let minimum_identity_width = format!("GPU {}", gpu.index).chars().count();
        let available_for_meter =
            (inner.width as usize).saturating_sub(minimum_identity_width + 1 + base_capacity_width);
        let meter_width = if desired_meter_width == 0 {
            0
        } else {
            desired_meter_width.min(available_for_meter.saturating_sub(1))
        };
        let capacity_width = base_capacity_width + usize::from(meter_width > 0) + meter_width;
        let identity_width = inner.width.saturating_sub(capacity_width as u16 + 1);
        let identity_area = Rect::new(inner.x, y, identity_width, 1);
        let capacity_area = Rect::new(
            inner.right().saturating_sub(capacity_width as u16),
            y,
            capacity_width.min(inner.width as usize) as u16,
            1,
        );

        frame.render_widget(
            Paragraph::new(gpu_identity_line(gpu, identity_width as usize, colors)),
            identity_area,
        );
        let mut capacity_spans = vec![
            Span::styled(memory_value, Style::default().fg(colors.text)),
            Span::raw("  "),
            Span::styled(
                percent,
                Style::default()
                    .fg(gpu_memory_color(memory_pct, colors))
                    .add_modifier(Modifier::BOLD),
            ),
        ];
        if meter_width > 0 {
            capacity_spans.push(Span::raw(" "));
            capacity_spans.extend(gpu_memory_meter(memory_pct, meter_width, colors));
        }
        frame.render_widget(Paragraph::new(Line::from(capacity_spans)), capacity_area);
        y += 1;

        if window.lines_per_gpu >= 2 && y < inner.bottom() {
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
        if window.lines_per_gpu == 3 && y < inner.bottom() {
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
        panel_title("SERVER METRICS", colors.title),
        panel_value("live + lifetime · c cache ".to_string(), colors.dim),
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
    let [cache_reuse, cache_context] = cache_overview_metrics(app, colors);
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
            "GEN LIVE",
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
        cache_reuse,
        cache_context,
        ("EVAL TOK", fmt_num(app.total_prompt_tokens), colors.prompt),
        (
            "GENERATED",
            fmt_num(app.total_predict_tokens),
            colors.predict,
        ),
        ("DECODES", fmt_num(metrics.n_decode_total), colors.bright),
        (
            "GEN AVG",
            format!("{:.1} tok/s", metrics.predicted_tokens_seconds),
            colors.predict,
        ),
        (
            "SPEC ENABLED",
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

fn cache_overview_metrics(app: &App, colors: ThemeColors) -> [(&'static str, String, Color); 2] {
    let active: Vec<&CacheRequestObservation> = app.active_cache_requests().collect();
    if !active.is_empty() {
        let reused = active.iter().fold(0_u64, |total, request| {
            total.saturating_add(request.reused_tokens)
        });
        let evaluated = active.iter().fold(0_u64, |total, request| {
            total.saturating_add(request.evaluated_tokens)
        });
        let context = active.iter().fold(0_u64, |total, request| {
            total.saturating_add(request.context_tokens)
        });
        let capacity = active.iter().fold(0_u64, |total, request| {
            total.saturating_add(request.context_capacity)
        });
        let input = reused.saturating_add(evaluated);
        let reuse_percent = if input > 0 {
            reused as f64 / input as f64 * 100.0
        } else {
            0.0
        };
        let provisional = active.iter().any(|request| request.provisional());
        let reuse_label = if app.snapshot.slots_error.is_some() {
            "REUSE STALE"
        } else if provisional {
            "REUSE NOW ~"
        } else {
            "REUSE NOW"
        };
        let reuse_value = if input > 0 {
            format!("{reuse_percent:.0}% · {}", fmt_num(reused as f64))
        } else {
            "waiting".to_string()
        };
        let context_percent = if capacity > 0 {
            context as f64 / capacity as f64 * 100.0
        } else {
            0.0
        };
        let headroom = if capacity > 0 {
            fmt_num(capacity.saturating_sub(context) as f64)
        } else {
            "—".to_string()
        };

        return [
            (
                reuse_label,
                reuse_value,
                if app.snapshot.slots_error.is_some() {
                    colors.error
                } else if input > 0 {
                    app.theme.cache.at(reuse_percent)
                } else {
                    colors.dim
                },
            ),
            (
                "CTX FREE",
                headroom,
                if capacity > 0 {
                    app.theme.memory.at(context_percent)
                } else {
                    colors.dim
                },
            ),
        ];
    }

    if let Some(last) = app.last_cache_request() {
        return [
            (
                "REUSE LAST",
                format!(
                    "{:.0}% · {}",
                    last.reuse_percent(),
                    fmt_num(last.reused_tokens as f64)
                ),
                if app.snapshot.slots_error.is_some() {
                    colors.warn
                } else {
                    app.theme.cache.at(last.reuse_percent())
                },
            ),
            (
                "CTX FREE",
                last.context_headroom()
                    .map_or_else(|| "—".to_string(), |tokens| fmt_num(tokens as f64)),
                if last.context_capacity > 0 {
                    app.theme.memory.at(last.context_percent())
                } else {
                    colors.dim
                },
            ),
        ];
    }

    let (context, capacity) =
        app.snapshot
            .slots
            .iter()
            .fold((0_u64, 0_u64), |(context_total, capacity_total), slot| {
                (
                    context_total.saturating_add(slot.context_tokens.max(0) as u64),
                    capacity_total.saturating_add(slot.context_capacity.max(0) as u64),
                )
            });
    let context_percent = if capacity > 0 {
        context as f64 / capacity as f64 * 100.0
    } else {
        0.0
    };

    [
        ("REUSE", "—".to_string(), colors.dim),
        (
            "CTX FREE",
            if capacity > 0 {
                fmt_num(capacity.saturating_sub(context) as f64)
            } else {
                "—".to_string()
            },
            if capacity > 0 {
                app.theme.memory.at(context_percent)
            } else {
                colors.dim
            },
        ),
    ]
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
    let scroll_hint = matches!(
        app.current_section,
        Section::Service | Section::Slots | Section::Cache | Section::Gpu
    ) && area.width >= 92;
    let show_interval_control = area.width >= 100;
    let compact_controls = area.width < 78;
    let controls_width = if compact_controls {
        if app.paused {
            16
        } else {
            12
        }
    } else {
        (if scroll_hint && app.paused {
            54
        } else if scroll_hint {
            52
        } else if app.paused {
            43
        } else {
            34
        }) + u16::from(show_interval_control) * 10
    };
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(24), Constraint::Length(controls_width)])
        .split(area);
    let full_labels = chunks[0].width >= 55;

    let mut tabs = vec![
        Span::raw(" "),
        Span::styled("Tab", key_style(colors)),
        Span::styled("  ", Style::default().fg(colors.dim)),
    ];
    for (section, long, short) in [
        (Section::Overview, "Overview", "OVR"),
        (Section::Service, "Service", "SVC"),
        (Section::Throughput, "Throughput", "RATE"),
        (Section::Slots, "Slots", "SLOT"),
        (Section::Cache, "Cache", "CACHE"),
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
    if compact_controls {
        if app.paused {
            controls.push(Span::styled("■ ", Style::default().fg(colors.warn)));
        }
        for (key, suffix) in [("p", "  "), ("t", "  "), ("?", "  "), ("q", "")] {
            controls.push(Span::styled(key, key_style(colors)));
            controls.push(Span::styled(suffix, Style::default().fg(colors.dim)));
        }
        frame.render_widget(
            Paragraph::new(Line::from(controls)).alignment(Alignment::Right),
            chunks[1],
        );
        return;
    }
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
    let popup = centered_fixed(area, area.width.min(78), area.height.min(29));
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
        help_row(
            "↑ / ↓",
            "Scroll service, slot, cache, and GPU details",
            colors,
        ),
        help_row("c", "Open prompt reuse and cache details", colors),
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
            "Lane activity, context use, evaluated/reused input, and output",
            colors,
        ),
        help_row("/props", "Model, build, slots, and context size", colors),
        help_row(
            "/v1/models",
            "Parameter count, model size, and trained context",
            colors,
        ),
        help_row(
            "nvidia-smi",
            "GPU load, memory, temperature, and power",
            colors,
        ),
        help_row(
            "/proc (local)",
            "Server uptime, launch configuration, memory, and host load",
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

fn gpu_identity_line(gpu: &GpuInfo, width: usize, colors: ThemeColors) -> Line<'static> {
    let id = format!("GPU {}", gpu.index);
    let name = shortened_gpu_name(&gpu.name);
    let show_name = !name.is_empty() && id.chars().count() + name.chars().count() + 2 <= width;

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

fn gpu_memory_color(percent: f64, colors: ThemeColors) -> Color {
    if percent > 95.0 {
        colors.error
    } else if percent >= 90.0 {
        colors.warn
    } else {
        colors.memory
    }
}

fn gpu_memory_meter(percent: f64, width: usize, colors: ThemeColors) -> Vec<Span<'static>> {
    let percent = percent.clamp(0.0, 100.0);
    let filled = (percent / 100.0 * width as f64).round() as usize;
    let fill_color = gpu_memory_color(percent, colors);
    (0..width)
        .map(|index| {
            if index < filled {
                Span::styled("▪", Style::default().fg(fill_color))
            } else {
                Span::styled("·", Style::default().fg(colors.track))
            }
        })
        .collect()
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

fn path_basename(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_string()
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

fn fmt_optional_f64(value: Option<f64>) -> String {
    value.map_or_else(
        || "—".to_string(),
        |value| {
            if value.fract().abs() < f64::EPSILON {
                format!("{value:.0}")
            } else {
                format!("{value:.2}")
            }
        },
    )
}

fn fmt_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    const TIB: f64 = GIB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= TIB {
        format!("{:.2} TiB", bytes / TIB)
    } else if bytes >= GIB {
        format!("{:.1} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}

fn fmt_kib(kib: u64) -> String {
    fmt_bytes(kib.saturating_mul(1024))
}

fn fmt_duration(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = seconds % 86_400 / 3_600;
    let minutes = seconds % 3_600 / 60;
    let seconds = seconds % 60;
    if days > 0 {
        format!("{days}d {hours:02}h {minutes:02}m")
    } else if hours > 0 {
        format!("{hours}h {minutes:02}m {seconds:02}s")
    } else {
        format!("{minutes:02}m {seconds:02}s")
    }
}

fn fmt_seconds(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        "—".to_string()
    } else {
        fmt_duration(seconds.round() as u64)
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
    use crate::api::{
        ChatCapabilities, GpuInfo, HostInfo, LocalServerInfo, Metrics, ModelInfo, RequestParams,
        ServerProps, SlotInfo,
    };
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
            prompt_seconds_total: 197.0,
            tokens_predicted_seconds_total: 612.0,
            prompt_tokens_seconds: 358.8,
            predicted_tokens_seconds: 47.1,
            requests_processing: 1.0,
            requests_deferred: 2.0,
            n_busy_slots_per_decode: 1.4,
        };
        app.snapshot.props = Some(ServerProps {
            model_alias: "deepseek-v4-flash-iq4".to_string(),
            model_ftype: "IQ4_XS".to_string(),
            total_slots: 2,
            n_ctx: 262_144,
            build_info: "b6100".to_string(),
            endpoint_slots: Some(true),
            endpoint_metrics: Some(true),
            ui_enabled: Some(false),
            chat_capabilities: ChatCapabilities {
                tools: true,
                parallel_tool_calls: true,
                system_role: true,
                ..ChatCapabilities::default()
            },
            default_generation: RequestParams {
                temperature: Some(1.0),
                top_k: Some(40),
                top_p: Some(1.0),
                min_p: Some(0.0),
                ..RequestParams::default()
            },
            ..ServerProps::default()
        });
        app.snapshot.model = Some(ModelInfo {
            id: "deepseek-v4-flash-iq4".to_string(),
            format: "gguf".to_string(),
            parameter_count: 284_334_567_511,
            size_bytes: 136_657_101_148,
            context_size: 262_144,
            trained_context_size: 1_048_576,
            embedding_size: 4096,
            vocabulary_size: 129_280,
            ftype: "IQ4_XS - 4.25 bpw".to_string(),
        });
        app.snapshot.local_server = Some(LocalServerInfo {
            pid: 4242,
            binary_path: "/opt/llama-server".to_string(),
            bind_host: "0.0.0.0".to_string(),
            port: 8080,
            process_uptime_seconds: Some(300_000),
            rss_kib: Some(9_500_000),
            threads: Some(90),
            cgroup_memory_current: Some(14 * 1024 * 1024 * 1024),
            cgroup_memory_limit: Some(30 * 1024 * 1024 * 1024),
            cgroup_swap_limit: Some(1024 * 1024 * 1024),
            draft_model: "dspark-q8.gguf".to_string(),
            devices: "CUDA0,CUDA1,CUDA2,CUDA3".to_string(),
            split_mode: "layer".to_string(),
            parallel: Some(2),
            speculative_type: "draft-dspark".to_string(),
            speculative_max_tokens: Some(3),
            batch_size: Some(2048),
            ubatch_size: Some(512),
            cache_ram_mib: Some(2048),
            cache_type_k: "f16".to_string(),
            cache_type_v: "f16".to_string(),
            flash_attention: Some(true),
            web_ui_enabled: Some(false),
            api_key_configured: false,
        });
        app.snapshot.host = Some(HostInfo {
            memory_total_kib: 32 * 1024 * 1024,
            memory_available_kib: 12 * 1024 * 1024,
            swap_total_kib: 8 * 1024 * 1024,
            swap_free_kib: 8 * 1024 * 1024,
            load_one: 2.1,
            load_five: 1.8,
            load_fifteen: 1.5,
            logical_cpus: 32,
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
                remaining_tokens: Some(31_000),
                has_next_token: Some(true),
                params: RequestParams {
                    max_tokens: Some(32_768),
                    temperature: Some(1.0),
                    top_k: Some(40),
                    top_p: Some(1.0),
                    min_p: Some(0.0),
                    stream: Some(true),
                    chat_format: "peg-native".to_string(),
                    reasoning_format: "deepseek".to_string(),
                    speculative_types: "draft-dspark".to_string(),
                },
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
        let cache_slots = app.snapshot.slots.clone();
        app.observe_cache_slots(&cache_slots, std::time::Instant::now());
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
            "SERVER METRICS",
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
        assert!(output.contains("UTIL 42%   TEMP 58°C   POWER 255/450W"));
        assert!(output.contains("GEN AVG"));
        assert!(output.contains("47.1 tok/s"));
        assert!(output.contains("REUSE NOW"));
        assert!(output.contains("82% · 10.2K"));
        assert!(output.contains("CTX FREE"));
        assert!(output.contains("249.3K"));
        assert!(output.contains("c cache"));
        assert!(!output.contains("TOK/DECODE"));
        assert!(!output.contains("SPEC READY"));
    }

    #[test]
    fn compact_overview_keeps_cache_summary_and_system_charts() {
        let mut app = demo_app();
        app.current_section = Section::Overview;
        let output = render(&app, 60, 20);

        for expected in [
            "SERVER METRICS",
            "REUSE NOW",
            "82% · 10.2K",
            "CTX FREE",
            "249.3K",
            "GPU UTIL",
            "POWER",
            "c cache",
        ] {
            assert!(
                output.contains(expected),
                "missing {expected:?} from compact overview"
            );
        }
    }

    #[test]
    fn overview_cache_summary_retains_the_last_request_while_idle() {
        let mut app = demo_app();
        app.observe_cache_slots(&[], std::time::Instant::now());
        for slot in &mut app.snapshot.slots {
            slot.is_processing = false;
        }
        app.current_section = Section::Overview;
        let output = render(&app, 80, 24);

        assert!(output.contains("REUSE LAST"));
        assert!(output.contains("82% · 10.2K"));
        assert!(output.contains("CTX FREE"));
        assert!(output.contains("249.3K"));
    }

    #[test]
    fn overview_uses_retained_slot_context_before_observing_a_request() {
        let fixture = demo_app();
        let mut app = App::new("http://127.0.0.1:8080".to_string());
        app.snapshot = fixture.snapshot;
        for slot in &mut app.snapshot.slots {
            slot.is_processing = false;
        }
        app.current_section = Section::Overview;
        let output = render(&app, 60, 20);

        assert!(output.contains("REUSE                     —"));
        assert!(output.contains("CTX FREE              421.2K"));
    }

    #[test]
    fn service_view_exposes_identity_runtime_workload_and_request_context() {
        let mut app = demo_app();
        app.current_section = Section::Service;
        let output = render(&app, 120, 36);

        for expected in [
            "SERVICE & WORKLOAD",
            "284.3B params",
            "1.0M trained",
            "dspark-q8.gguf",
            "draft-dspark · max 3",
            "0.0.0.0:8080 · API key not set",
            "PID 4242",
            "up 3d 11h 20m",
            "1/2 active · 2 queued",
            "49.1 live",
            "47.1",
            "age and client are not exposed",
            "42 · decode · streaming",
            "31.0K remain",
            "acceptance is not exposed",
        ] {
            assert!(
                output.contains(expected),
                "missing {expected:?} from service view"
            );
        }
        assert!(!output.contains("PROMPT EVAL"));
    }

    #[test]
    fn service_view_does_not_claim_speculation_for_an_ordinary_local_server() {
        let mut app = demo_app();
        let server = app
            .snapshot
            .local_server
            .as_mut()
            .expect("local server fixture");
        server.draft_model.clear();
        server.speculative_type = "none".to_string();
        server.speculative_max_tokens = None;
        for slot in &mut app.snapshot.slots {
            slot.speculative = false;
            slot.params.speculative_types = "none".to_string();
        }
        app.current_section = Section::Service;
        let output = render(&app, 120, 36);

        assert!(output.contains("SPEC         off"));
        assert!(!output.contains("SPEC         configured"));
    }

    #[test]
    fn minimum_layout_keeps_every_section_tab_reachable() {
        let mut app = demo_app();
        app.current_section = Section::Service;
        let output = render(&app, 60, 20);

        for tab in ["OVR", "SVC", "RATE", "SLOT", "CACHE", "GPU"] {
            assert!(output.contains(tab), "missing compact {tab} tab");
        }
    }

    #[test]
    fn cache_view_explains_live_reuse_context_scope_and_configuration() {
        let mut app = demo_app();
        app.current_section = Section::Cache;
        let output = render(&app, 120, 36);

        for expected in [
            "CACHE",
            "PROMPT REUSE — CURRENT",
            "task 42 · slot 0 · decode · settled",
            "82%",
            "12.4K = 10.2K reused + 2.2K evaluated",
            "249.3K tokens · 12.8K / 262.1K occupied",
            "OBSERVED SINCE LTOP START",
            "1 observed",
            "active + completed tasks seen",
            "SLOT CACHE",
            "REUSE",
            "CACHE CONFIGURATION",
            "K f16 / V f16",
            "current use not exposed",
            "actual KV bytes, entries, evictions",
        ] {
            assert!(
                output.contains(expected),
                "missing {expected:?} from cache view"
            );
        }
        assert!(!output.contains("hit rate"));
        let reuse_bar = output
            .lines()
            .find(|line| line.contains("REUSE        ["))
            .expect("prompt reuse bar");
        assert!(
            reuse_bar.contains("]  82%"),
            "reuse bar should fit its wide-layout column: {reuse_bar}"
        );
        let context_bar = output
            .lines()
            .find(|line| line.contains("CONTEXT      ["))
            .expect("context occupancy bar");
        assert!(context_bar.contains("]   5%"));
    }

    #[test]
    fn cache_view_retains_the_last_observed_request_while_idle() {
        let mut app = demo_app();
        app.observe_cache_slots(&[], std::time::Instant::now());
        for slot in &mut app.snapshot.slots {
            slot.is_processing = false;
        }
        app.current_section = Section::Cache;
        let output = render(&app, 80, 24);

        assert!(output.contains("PROMPT REUSE — LAST OBSERVED"));
        assert!(output.contains("task 42 · slot 0 · decode · just now"));
        assert!(output.contains("82%"));
    }

    #[test]
    fn cache_view_marks_retained_data_stale_when_slot_polling_fails() {
        let mut app = demo_app();
        app.snapshot.slots.clear();
        app.snapshot.slots_error = Some("cannot reach /slots: timed out".to_string());
        app.current_section = Section::Cache;
        let output = render(&app, 120, 36);

        assert!(output.contains("PROMPT REUSE — LAST SAMPLE"));
        assert!(output.contains("telemetry stale"));
        assert!(output.contains("cannot reach /slots: timed out"));
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
        assert!(output.contains("UTIL 42%   TEMP 58°C   POWER 255/450W"));
        assert!(!output.contains("VRAM "));
    }

    #[test]
    fn gpu_memory_panel_puts_a_short_dot_meter_on_the_capacity_row() {
        let mut app = demo_app();
        app.current_section = Section::Gpu;
        let output = render(&app, 80, 24);
        let lines: Vec<&str> = output.lines().collect();
        let gpu_row = lines
            .iter()
            .position(|line| line.contains("GPU 0"))
            .expect("GPU 0 row");

        assert!(lines[gpu_row].contains("20.0 / 24.0 GiB"));
        assert!(lines[gpu_row].contains("83%"));
        assert!(lines[gpu_row].contains("▪▪▪▪▪▪▪▪··"));
        assert!(lines[gpu_row + 1].contains("UTIL 42%   TEMP 58°C   POWER 255/450W"));
        assert!(!lines[gpu_row + 1].contains('█'));
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
    fn gradient_progress_bar_uses_the_theme_gradient_and_a_quiet_track() {
        let app = demo_app();
        let colors = app.theme.colors();
        let spans = gradient_progress_bar(50.0, 8, &app.theme.memory, colors.track);
        let symbols: String = spans.iter().map(|span| span.content.as_ref()).collect();

        assert_eq!(symbols, "████░░░░");
        assert_ne!(spans[0].style.fg, spans[3].style.fg);
        assert_eq!(spans.last().unwrap().style.fg, Some(colors.track));
    }

    #[test]
    fn gpu_memory_meter_uses_compact_dots() {
        let colors = demo_app().theme.colors();
        let spans = gpu_memory_meter(50.0, 10, colors);
        let symbols: String = spans.iter().map(|span| span.content.as_ref()).collect();

        assert_eq!(symbols, "▪▪▪▪▪·····");
        assert!(spans[..5]
            .iter()
            .all(|span| span.style.fg == Some(colors.memory)));
        assert!(spans[5..]
            .iter()
            .all(|span| span.style.fg == Some(colors.track)));
    }

    #[test]
    fn gpu_memory_meter_reserves_alert_colors_for_high_pressure() {
        let colors = demo_app().theme.colors();
        let normal = gpu_memory_meter(89.9, 10, colors);
        let warning = gpu_memory_meter(95.0, 10, colors);
        let critical = gpu_memory_meter(95.1, 10, colors);

        assert_eq!(normal[0].style.fg, Some(colors.memory));
        assert_eq!(warning[0].style.fg, Some(colors.warn));
        assert_eq!(critical[0].style.fg, Some(colors.error));
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
    fn saturated_generate_history_reaches_the_left_edge_of_a_focused_chart() {
        let mut app = demo_app();
        app.current_section = Section::Throughput;
        app.predict_rate_history.clear();
        app.predict_rate_history
            .extend(std::iter::repeat_n(50.0, MAX_SAMPLES));
        let output = render(&app, 80, 24);
        let lines: Vec<&str> = output.lines().collect();
        let title_row = lines
            .iter()
            .position(|line| line.contains("GENERATE"))
            .expect("generate chart title");
        let border_row = lines
            .iter()
            .enumerate()
            .skip(title_row + 1)
            .find_map(|(index, line)| line.starts_with('╰').then_some(index))
            .expect("generate chart bottom border");
        let graph = lines[border_row - 1]
            .split_once("0.0 ")
            .expect("bottom-axis label")
            .1;
        let first_graph_cell = graph.chars().next().expect("graph content");

        assert!(
            ('\u{2800}'..='\u{28ff}').contains(&first_graph_cell),
            "expected history at the left edge, got {first_graph_cell:?}"
        );
    }

    #[test]
    fn slots_section_explains_lane_activity_without_stale_idle_counters() {
        let mut app = demo_app();
        app.current_section = Section::Slots;
        let output = render(&app, 80, 24);

        for heading in ["CONTEXT", "EVAL", "REUSE", "OUTPUT"] {
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
        assert!(output.contains("Open prompt reuse and cache details"));
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
