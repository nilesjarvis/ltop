// Headless screenshot test for ltop

mod api;
mod app;
mod chart;
mod collect;
mod theme;
mod ui;

fn main() {
    let url = api::detect_server().unwrap_or_else(|| "http://127.0.0.1:8080".to_string());
    // Keep the headless fixture quick while exercising the same universal
    // cadence used by the interactive app.
    let mut app = app::App::with_update_ms(url, 500);

    // Data is fetched on a background thread; start it so poll() has
    // snapshots to fold into history.
    let _collector = app.start_collection();

    // Poll several times to build up history
    for _ in 0..15 {
        app.poll();
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    // Generate some test requests to populate throughput data
    for _ in 0..3 {
        let _ = ureq::post(&format!("{}/v1/completions", app.url))
            .timeout(std::time::Duration::from_secs(10))
            .set("Content-Type", "application/json")
            .send_string(r#"{"prompt":"Hello world test","max_tokens":20,"stream":false}"#);
        app.poll();
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    // Final poll
    app.poll();

    // Render to a buffer using ratatui's TestBackend
    let backend = ratatui::backend::TestBackend::new(140, 45);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|frame| ui::draw(frame, &app)).unwrap();

    // Get the buffer and print it
    let buffer = terminal.backend().buffer();
    let mut output = String::new();
    for y in 0..45 {
        for x in 0..140 {
            let cell = &buffer[(x, y)];
            output.push_str(cell.symbol());
        }
        output.push('\n');
    }

    print!("{}", output);
}
