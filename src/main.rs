use std::env;
use std::path::PathBuf;
use std::time::Duration;

mod api;
mod app;
mod chart;
mod theme;
mod ui;

use app::App;
use theme::{parse_update_ms, ThemeCatalog, ThemePreferences};

const HELP: &str = r#"ltop — a btop-inspired llama.cpp monitor

Usage: ltop [OPTIONS] [URL]

Arguments:
  [URL]                   llama.cpp server URL (auto-detected when omitted)

Options:
  -u, --update <MS>       Set the universal polling interval in milliseconds
      --theme <NAME|PATH> Use a discovered theme or a btop .theme file
      --themes-dir <DIR>  Search an additional theme directory first
      --list-themes       Print discovered themes and exit
      --transparent       Keep the terminal's background color
  -h, --help              Print help
  -V, --version           Print version

Inside ltop, use -/+ to change polling by 100 ms and t to preview themes.
"#;

#[derive(Debug, Default, PartialEq, Eq)]
struct Cli {
    url: Option<String>,
    update_ms: Option<u64>,
    theme: Option<String>,
    themes_dir: Option<PathBuf>,
    list_themes: bool,
    transparent: bool,
    help: bool,
    version: bool,
}

fn main() {
    if let Err(error) = start() {
        eprintln!("ltop: {error}");
        std::process::exit(1);
    }
}

fn start() -> Result<(), Box<dyn std::error::Error>> {
    let cli = parse_args(env::args().skip(1))?;
    if cli.help {
        print!("{HELP}");
        return Ok(());
    }
    if cli.version {
        println!("ltop {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if let Some(directory) = cli.themes_dir.as_deref() {
        if !directory.is_dir() {
            return Err(format!(
                "theme directory {} does not exist or is not a directory",
                directory.display()
            )
            .into());
        }
    }

    let preferences = ThemePreferences::load().unwrap_or_else(|error| {
        eprintln!("ltop: configuration warning: {error}");
        ThemePreferences::default()
    });
    let mut theme_catalog = ThemeCatalog::discover(cli.themes_dir.as_deref());
    for warning in theme_catalog.warnings() {
        eprintln!("ltop: theme warning: {warning}");
    }

    if cli.list_themes {
        for name in theme_catalog.names() {
            println!("{name}");
        }
        return Ok(());
    }

    let environment_theme = env::var("LTOP_THEME")
        .ok()
        .filter(|theme| !theme.trim().is_empty());
    let explicit_theme = cli.theme.is_some() || environment_theme.is_some();
    let requested_theme = cli
        .theme
        .as_deref()
        .or(environment_theme.as_deref())
        .unwrap_or(&preferences.theme);
    let active_theme_index = match theme_catalog.resolve(requested_theme) {
        Ok(index) => index,
        Err(error) if explicit_theme => {
            return Err(format!("{error}; run 'ltop --list-themes' to see available names").into())
        }
        Err(error) => {
            eprintln!("ltop: theme warning: {error}; using ltop");
            theme_catalog.index_of("ltop").unwrap_or(0)
        }
    };
    let theme_background = !cli.transparent && preferences.theme_background;
    let update_ms = cli.update_ms.unwrap_or(preferences.update_ms);

    let url = if let Some(url) = cli.url {
        url.trim_end_matches('/').to_string()
    } else if let Some(url) = api::detect_server() {
        url
    } else {
        eprintln!("ltop: could not auto-detect a llama.cpp server.");
        eprintln!("Make sure a llama-server is running (for example on port 8080).");
        eprintln!("You can also specify: ltop http://host:port");
        std::process::exit(1);
    };

    eprintln!("ltop: connecting to {url} ...");
    let app = App::with_theme_catalog_and_update_ms(
        url,
        theme_catalog,
        active_theme_index,
        theme_background,
        update_ms,
    );

    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        std::io::stderr(),
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = ratatui::backend::CrosstermBackend::new(std::io::stderr());
    let terminal = ratatui::Terminal::new(backend)?;

    let result = run(terminal, app);

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        std::io::stderr(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;

    match result {
        Ok(Some(warning)) => eprintln!("ltop: configuration warning: {warning}"),
        Ok(None) => {}
        Err(error) => return Err(error),
    }
    Ok(())
}

fn parse_args<I>(args: I) -> Result<Cli, String>
where
    I: IntoIterator,
    I::Item: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    let mut cli = Cli::default();
    let mut index = 0;
    let mut positional_only = false;

    while index < args.len() {
        let argument = &args[index];
        if positional_only {
            set_url(&mut cli, argument)?;
        } else if argument == "--" {
            positional_only = true;
        } else if matches!(argument.as_str(), "-h" | "--help") {
            cli.help = true;
        } else if matches!(argument.as_str(), "-V" | "--version") {
            cli.version = true;
        } else if argument == "--list-themes" {
            cli.list_themes = true;
        } else if argument == "--transparent" {
            cli.transparent = true;
        } else if matches!(argument.as_str(), "-u" | "--update") {
            index += 1;
            let value = required_value(&args, index, argument)?;
            cli.update_ms = Some(parse_update_ms(value)?);
        } else if let Some(value) = argument.strip_prefix("--update=") {
            if value.is_empty() {
                return Err("--update requires a value".to_string());
            }
            cli.update_ms = Some(parse_update_ms(value)?);
        } else if argument == "--theme" {
            index += 1;
            cli.theme = Some(required_value(&args, index, "--theme")?.to_string());
        } else if let Some(value) = argument.strip_prefix("--theme=") {
            if value.is_empty() {
                return Err("--theme requires a value".to_string());
            }
            cli.theme = Some(value.to_string());
        } else if argument == "--themes-dir" {
            index += 1;
            cli.themes_dir = Some(PathBuf::from(required_value(&args, index, "--themes-dir")?));
        } else if let Some(value) = argument.strip_prefix("--themes-dir=") {
            if value.is_empty() {
                return Err("--themes-dir requires a value".to_string());
            }
            cli.themes_dir = Some(PathBuf::from(value));
        } else if argument.starts_with('-') {
            return Err(format!("unknown option {argument:?}\nTry 'ltop --help'."));
        } else {
            set_url(&mut cli, argument)?;
        }
        index += 1;
    }

    Ok(cli)
}

fn required_value<'a>(args: &'a [String], index: usize, option: &str) -> Result<&'a str, String> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("{option} requires a value"))
}

fn set_url(cli: &mut Cli, value: &str) -> Result<(), String> {
    if cli.url.is_some() {
        return Err(format!("unexpected extra argument {value:?}"));
    }
    cli.url = Some(value.to_string());
    Ok(())
}

fn run(
    mut terminal: ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stderr>>,
    mut app: App,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};

    let mut configuration_warning = None;
    app.poll();

    loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        // Like btop, input waits only until the next universal collection
        // deadline (with a one-second wake-up cap for UI-only updates).
        let input_wait = app.poll_wait().min(Duration::from_secs(1));
        if crossterm::event::poll(input_wait)? {
            if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                if key.kind != KeyEventKind::Release {
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        break;
                    }

                    if app.show_theme_picker {
                        match key.code {
                            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                                app.preview_previous_theme()
                            }
                            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                                app.preview_next_theme()
                            }
                            KeyCode::PageUp => app.move_theme_preview(-5),
                            KeyCode::PageDown => app.move_theme_preview(5),
                            KeyCode::Home => app.preview_first_theme(),
                            KeyCode::End => app.preview_last_theme(),
                            KeyCode::Char('b') | KeyCode::Char('B') => {
                                app.toggle_theme_background()
                            }
                            KeyCode::Enter => {
                                if let Err(error) = app.commit_theme_picker() {
                                    configuration_warning = Some(error);
                                }
                            }
                            KeyCode::Esc
                            | KeyCode::Char('q')
                            | KeyCode::Char('Q')
                            | KeyCode::Char('t')
                            | KeyCode::Char('T') => app.cancel_theme_picker(),
                            _ => {}
                        }
                    } else {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Char('Q') => break,
                            KeyCode::Esc if app.show_help => app.toggle_help(),
                            KeyCode::Tab => app.next_section(),
                            KeyCode::BackTab => app.prev_section(),
                            KeyCode::Up => app.scroll_up(),
                            KeyCode::Down => app.scroll_down(),
                            KeyCode::Char('r') | KeyCode::Char('R') => app.toggle_rate_unit(),
                            KeyCode::Char('p') | KeyCode::Char('P') => app.toggle_pause(),
                            KeyCode::Char('-') => {
                                if let Err(error) = app.decrease_update_interval() {
                                    configuration_warning = Some(error);
                                }
                            }
                            KeyCode::Char('+') | KeyCode::Char('=') => {
                                if let Err(error) = app.increase_update_interval() {
                                    configuration_warning = Some(error);
                                }
                            }
                            KeyCode::Char('h') | KeyCode::Char('H') | KeyCode::Char('?') => {
                                app.toggle_help()
                            }
                            KeyCode::Char('t') | KeyCode::Char('T') => app.open_theme_picker(),
                            _ => {}
                        }
                    }
                }
            }
        }

        app.poll();
    }

    Ok(configuration_warning)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_accepts_theme_options_around_the_url() {
        let cli = parse_args([
            "--update=750",
            "--theme",
            "Nord",
            "http://localhost:8080/",
            "--themes-dir=./palettes",
            "--transparent",
        ])
        .unwrap();

        assert_eq!(cli.url.as_deref(), Some("http://localhost:8080/"));
        assert_eq!(cli.update_ms, Some(750));
        assert_eq!(cli.theme.as_deref(), Some("Nord"));
        assert_eq!(cli.themes_dir, Some(PathBuf::from("./palettes")));
        assert!(cli.transparent);
    }

    #[test]
    fn cli_rejects_unknown_options_and_extra_urls() {
        assert!(parse_args(["--unknown"]).is_err());
        assert!(parse_args(["one", "two"]).is_err());
        assert!(parse_args(["--theme"]).is_err());
        assert!(parse_args(["--update"]).is_err());
        assert!(parse_args(["--update", "99"]).is_err());
        assert!(parse_args(["--update", "86400001"]).is_err());
        assert!(parse_args(["--update", "fast"]).is_err());
    }

    #[test]
    fn cli_accepts_btop_style_short_update_option() {
        let cli = parse_args(["-u", "2000"]).unwrap();

        assert_eq!(cli.update_ms, Some(2_000));
    }
}
