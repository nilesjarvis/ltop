# 🦙 ltop

**A btop-inspired terminal monitor for llama.cpp servers.**

ltop combines llama.cpp request telemetry with host GPU statistics in a responsive terminal UI. It shows prompt-evaluation and generation throughput, slot activity, context and cache use, GPU memory, utilization, temperature, power, and session totals without requiring a browser.

The interface uses one configurable polling cadence, supports btop-compatible themes, and adapts from a complete dashboard to focused section views on smaller terminals.

## Screenshots

![ltop dashboard](screenshots/ltop-dashboard.png)

*The ltop dashboard monitoring a llama.cpp server.*


## Highlights

- Server-timed prompt-evaluation throughput and live generation throughput
- Busy/idle slot lanes with context, evaluated input, cache reuse, and output
- Multi-GPU memory meters plus utilization, temperature, and power telemetry
- High-resolution Braille history charts inspired by btop
- One polling interval for `/metrics`, `/slots`, `/props`, and `nvidia-smi`
- Responsive full-dashboard and focused-section layouts
- Bundled palettes, btop `.theme` compatibility, and live theme previews
- Automatic local llama.cpp server discovery or an explicit remote URL

## Requirements

- A current stable Rust toolchain to build ltop
- A running `llama-server` with the metrics and slots endpoints enabled
- A terminal at least `60×20`; Unicode and true-color support are recommended
- `nvidia-smi` for NVIDIA GPU telemetry; ltop remains usable without it

For example, start llama.cpp with the endpoints ltop consumes:

```bash
llama-server \
  --model /path/to/model.gguf \
  --host 127.0.0.1 \
  --port 8080 \
  --metrics \
  --slots
```

The exact llama.cpp arguments available to you depend on your server build.

## Installation

From a repository checkout, install the `ltop` binary into Cargo's binary directory:

```bash
cargo install --path . --bin ltop
```

Alternatively, build and run it directly:

```bash
cargo build --release
./target/release/ltop
```

The included helper performs the same release build:

```bash
./install.sh
./target/release/ltop
```

## Getting started

When no URL is supplied, ltop looks for a local `llama-server` process and then checks common localhost ports:

```bash
ltop
```

Pass a URL to monitor a specific local or remote server:

```bash
ltop http://127.0.0.1:8080
ltop http://inference-host:8080
```

Options can appear before or after the URL. This example selects a theme and polls every second:

```bash
ltop --theme "Tokyo Night" --update 1000 http://127.0.0.1:8080
```

Run `ltop --help` for the built-in command reference.

## Dashboard

On roomy terminals, ltop presents every panel together. On smaller terminals, `Tab` and `Shift-Tab` switch between focused views.

| Panel | What it shows |
|---|---|
| `PROMPT EVAL` | The latest completed prompt-evaluation speed and its polling timeline |
| `GENERATE` | Current decoded-token throughput derived from active slots |
| `SLOTS` | Lane state, retained context, evaluated and cached input, and output |
| `GPU MEMORY` | Aggregate and per-device VRAM usage with adaptive device details |
| `GPU UTIL` | Average GPU utilization history |
| `POWER` | Total GPU power history |
| `SESSION` | Request pressure, token totals, decode statistics, and speculative readiness |

The GPU memory view prioritizes capacity: each device receives an aligned usage value, percentage, and themed gradient meter. Utilization, temperature, power, and model identity are retained as secondary information when space allows. A range marker such as `3–6/8` appears when the device list is clipped.

## Keyboard controls

| Key | Action |
|---|---|
| `q` / `Ctrl-C` | Quit |
| `Tab` / `Shift-Tab` | Select the next or previous section |
| `↑` / `↓` | Scroll slot and GPU lists |
| `p` | Pause or resume collection |
| `-` / `+` | Decrease or increase polling by 100 ms |
| `t` | Open the live theme picker |
| `?` / `h` | Open or close help |
| `Esc` | Close help or cancel the theme picker |

Inside the theme picker:

| Key | Action |
|---|---|
| `↑` / `↓` or `k` / `j` | Preview the previous or next theme |
| `Page Up` / `Page Down` | Move five themes at a time |
| `Home` / `End` | Jump to the first or last theme |
| `b` | Toggle the theme background |
| `Enter` | Apply and save the previewed theme |
| `Esc`, `q`, or `t` | Cancel and restore the applied theme |

## Command-line reference

```text
Usage: ltop [OPTIONS] [URL]
```

| Option | Description |
|---|---|
| `[URL]` | llama.cpp server URL; auto-detected when omitted |
| `-u, --update <MS>` | Universal polling interval in milliseconds |
| `--theme <NAME or PATH>` | Discovered theme name or path to a btop `.theme` file |
| `--themes-dir <DIR>` | Search an additional theme directory first |
| `--list-themes` | Print discovered themes and exit |
| `--transparent` | Preserve the terminal's background color |
| `-h, --help` | Print help and exit |
| `-V, --version` | Print the version and exit |

The update interval accepts integer values from `100` through `86400000` ms. Its default is `2000` ms, matching btop's `update_ms` model.

## Polling and configuration

`/metrics`, `/slots`, `/props`, and `nvidia-smi` are collected as one snapshot on one deadline. There are no component-specific timers. When collection takes longer than the configured interval, ltop skips missed ticks instead of issuing a burst of catch-up requests.

Changes made with `-` and `+`, along with the applied theme and background preference, are saved to:

```text
$XDG_CONFIG_HOME/ltop/config
```

If `XDG_CONFIG_HOME` is unset, ltop uses `~/.config/ltop/config`.

```ini
# ltop preferences
theme = "ltop"
theme_background = true
update_ms = 2000
```

Pausing stops collection. Resuming starts with a fresh snapshot so time spent paused is not misreported as throughput.

## Themes

Press `t` for live previews. ltop includes these palettes:

- ltop
- Default
- TTY
- Tokyo Night
- Gruvbox Dark
- Nord
- Dracula
- Solarized Light

ltop understands btop `.theme` files and discovers them from:

- A directory passed with `--themes-dir`
- `$XDG_CONFIG_HOME/ltop/themes` or `~/.config/ltop/themes`
- Installed ltop theme directories under `/usr/local/share` and `/usr/share`
- `$XDG_CONFIG_HOME/btop/themes` or `~/.config/btop/themes`
- Installed btop theme directories under `/usr/local/share` and `/usr/share`

Theme names are case-insensitive, and spaces, underscores, and hyphens are interchangeable.

```bash
# List every discoverable theme
ltop --list-themes

# Select a discovered theme
ltop --theme Nord

# Load a theme file directly
ltop --theme ~/.config/btop/themes/onedark.theme

# Search a custom directory first
ltop --themes-dir ./themes --theme custom

# Select through the environment
LTOP_THEME="Gruvbox Dark" ltop
```

See [themes/README.md](themes/README.md) for notes about the bundled files and custom theme syntax.

## Data sources and metric semantics

| Source | Used for |
|---|---|
| `/metrics` | Prompt-processing counters and timing, request pressure, token totals, and decode statistics |
| `/slots` | Per-lane activity, context use, evaluated and cached input, output, and speculative capability |
| `/props` | Model identity, quantization, build information, slot count, and context size |
| `nvidia-smi` | GPU utilization, memory, temperature, and power telemetry |

### Prompt evaluation

`PROMPT EVAL` is based on llama.cpp's cumulative evaluated-token and active prompt-processing-time counters rather than HTTP scrape time:

- `last` is the token-counter delta divided by the prompt-time delta between two snapshots.
- `avg` is llama.cpp's cumulative average gauge, used on the initial connection or when millisecond counter rounding prevents a safe interval calculation.
- The headline retains the most recent valid measurement.
- The chart advances on every polling tick and records zero when no new prompt work completes.
- `EVAL TOK` excludes prompt tokens reused from cache.

This makes gaps between prefill bursts visible without allowing idle time or request latency to distort the measured speed.

### Slots and generation

Generation throughput uses changes in each active slot's decoded-token counter. ltop tracks lanes independently so a request transition or counter reset in one slot cannot erase progress from another.

Slot `CACHED` percentages are provisional while prompt evaluation is still running and settle when that phase completes. Values belonging to a previous task are not shown as current output on an idle lane.

## Troubleshooting

### The server is not detected

Pass its URL explicitly:

```bash
ltop http://host:port
```

Auto-detection is designed primarily for local Linux processes. An explicit URL is the most reliable choice for containers, remote servers, and other operating systems.

### Metrics or slots are unavailable

Confirm that your `llama-server` exposes `/metrics`, `/slots`, and `/props`, and that it was started with the corresponding metrics and slots options. ltop reports endpoint failures in the status and slot views instead of silently presenting an empty server.

### GPU telemetry is unavailable

Verify that `nvidia-smi` is installed, available in `PATH`, and can see the GPUs from the same environment where ltop runs. Server monitoring continues without GPU data.

### The interface is clipped or glyphs look wrong

Use a terminal of at least `60×20` and a font with Braille and emoji glyphs. Resize the terminal for the full dashboard or use `Tab` to navigate focused sections.

## Development

Run the complete local validation suite with:

```bash
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```
