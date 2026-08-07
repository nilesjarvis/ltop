#![allow(dead_code)]

use std::time::Instant;

#[derive(Debug, Clone, Default)]
pub struct Metrics {
    pub prompt_tokens_total: f64,
    pub prompt_seconds_total: f64,
    pub tokens_predicted_total: f64,
    pub tokens_predicted_seconds_total: f64,
    pub n_decode_total: f64,
    pub n_tokens_max: f64,
    pub prompt_tokens_seconds: f64,
    pub predicted_tokens_seconds: f64,
    pub requests_processing: f64,
    pub requests_deferred: f64,
    pub n_busy_slots_per_decode: f64,
}

#[derive(Debug, Clone, Default)]
pub struct SlotInfo {
    pub id: i64,
    pub task_id: Option<i64>,
    pub context_capacity: i64,
    pub speculative: bool,
    pub is_processing: bool,
    pub context_tokens: i64,
    pub prompt_tokens_processed: i64,
    pub prompt_tokens_cached: i64,
    pub decoded_tokens: i64,
}

impl SlotInfo {
    /// Accepted output currently retained in this slot's context.
    ///
    /// llama.cpp's `n_decoded` can briefly belong to the previous task while a
    /// new prompt is being prepared. Deriving this from the live context keeps
    /// the value tied to the request users see as busy.
    pub fn current_output_tokens(&self) -> i64 {
        if !self.is_processing {
            return 0;
        }

        (self.context_tokens - self.prompt_tokens_cached - self.prompt_tokens_processed).max(0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ServerProps {
    pub model_alias: String,
    pub model_path: String,
    pub model_ftype: String,
    pub total_slots: i64,
    pub n_ctx: i64,
    pub build_info: String,
    pub is_sleeping: bool,
}

#[derive(Debug, Clone, Default)]
pub struct GpuInfo {
    pub index: i32,
    pub name: String,
    pub gpu_util: f64,
    pub mem_used: u64,    // MiB
    pub mem_total: u64,   // MiB
    pub temp: f64,        // Celsius
    pub power_draw: f64,  // Watts
    pub power_limit: f64, // Watts
    pub clock_gr: i64,    // MHz
    pub clock_mem: i64,   // MHz
    pub fan_speed: f64,   // %
}

#[derive(Debug, Clone)]
pub struct Sample {
    pub timestamp: Instant,
    pub prompt_tok_s: f64,
    pub predict_tok_s: f64,
    pub requests_processing: f64,
    pub gpu_util: f64,   // avg across GPUs
    pub mem_pct: f64,    // avg across GPUs
    pub power_draw: f64, // total across GPUs
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub metrics: Metrics,
    pub metrics_available: bool,
    pub prev_metrics: Option<Metrics>,
    pub slots: Vec<SlotInfo>,
    pub props: Option<ServerProps>,
    pub gpus: Vec<GpuInfo>,
    pub last_request_timings: Option<RequestTimings>,
    pub connected: bool,
    pub error: Option<String>,
    pub slots_error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RequestTimings {
    pub prompt_n: i64,
    pub prompt_ms: f64,
    pub prompt_per_second: f64,
    pub predicted_n: i64,
    pub predicted_ms: f64,
    pub predicted_per_second: f64,
    pub draft_n: i64,
    pub draft_n_accepted: i64,
}

impl Snapshot {
    pub fn new() -> Self {
        Self::default()
    }
}

fn http_get_result(url: &str) -> Result<String, String> {
    match ureq::get(url)
        .timeout(std::time::Duration::from_secs(5))
        .call()
    {
        Ok(resp) => resp
            .into_string()
            .map_err(|error| format!("could not read response: {error}")),
        Err(ureq::Error::Status(code, _)) => Err(format!("HTTP {code}")),
        Err(ureq::Error::Transport(error)) => Err(error.to_string()),
    }
}

fn http_get(url: &str) -> Option<String> {
    http_get_result(url).ok()
}

fn parse_prometheus(text: &str) -> Metrics {
    let mut m = Metrics::default();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut fields = line.split_whitespace();
        if let (Some(key), Some(value)) = (fields.next(), fields.next()) {
            let val: f64 = value.parse().unwrap_or(0.0);
            // The metrics currently used by llama.cpp are unlabeled, but
            // accepting a Prometheus label set keeps the parser correct if a
            // server version adds one later.
            let name = key.split_once('{').map_or(key, |(name, _)| name);
            match name {
                "llamacpp:prompt_tokens_total" => m.prompt_tokens_total = val,
                "llamacpp:prompt_seconds_total" => m.prompt_seconds_total = val,
                "llamacpp:tokens_predicted_total" => m.tokens_predicted_total = val,
                "llamacpp:tokens_predicted_seconds_total" => m.tokens_predicted_seconds_total = val,
                "llamacpp:n_decode_total" => m.n_decode_total = val,
                "llamacpp:n_tokens_max" => m.n_tokens_max = val,
                "llamacpp:prompt_tokens_seconds" => m.prompt_tokens_seconds = val,
                "llamacpp:predicted_tokens_seconds" => m.predicted_tokens_seconds = val,
                "llamacpp:requests_processing" => m.requests_processing = val,
                "llamacpp:requests_deferred" => m.requests_deferred = val,
                "llamacpp:n_busy_slots_per_decode" => m.n_busy_slots_per_decode = val,
                _ => {}
            }
        }
    }
    m
}

fn parse_slots(json_text: &str) -> Result<Vec<SlotInfo>, String> {
    let value: serde_json::Value =
        serde_json::from_str(json_text).map_err(|error| format!("invalid JSON: {error}"))?;
    let slots = value
        .as_array()
        .ok_or_else(|| "expected a JSON array".to_string())?;

    Ok(slots
        .iter()
        .map(|slot| {
            let next_token = slot.get("next_token");
            let decoded_tokens = match next_token {
                Some(serde_json::Value::Array(tokens)) => tokens.first(),
                Some(serde_json::Value::Object(_)) => next_token,
                _ => None,
            }
            .and_then(|token| token.get("n_decoded"))
            .and_then(|value| value.as_i64())
            .unwrap_or(0);

            SlotInfo {
                id: slot.get("id").and_then(|value| value.as_i64()).unwrap_or(0),
                task_id: slot.get("id_task").and_then(|value| value.as_i64()),
                context_capacity: slot
                    .get("n_ctx")
                    .and_then(|value| value.as_i64())
                    .unwrap_or(0),
                speculative: slot
                    .get("speculative")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false),
                is_processing: slot
                    .get("is_processing")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false),
                context_tokens: slot
                    .get("n_prompt_tokens")
                    .and_then(|value| value.as_i64())
                    .unwrap_or(0),
                prompt_tokens_processed: slot
                    .get("n_prompt_tokens_processed")
                    .and_then(|value| value.as_i64())
                    .unwrap_or(0),
                prompt_tokens_cached: slot
                    .get("n_prompt_tokens_cache")
                    .and_then(|value| value.as_i64())
                    .unwrap_or(0),
                decoded_tokens,
            }
        })
        .collect())
}

fn parse_props(json_text: &str) -> Option<ServerProps> {
    let v: serde_json::Value = serde_json::from_str(json_text).ok()?;
    Some(ServerProps {
        model_alias: v
            .get("model_alias")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        model_path: v
            .get("model_path")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        model_ftype: v
            .get("model_ftype")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        total_slots: v.get("total_slots").and_then(|x| x.as_i64()).unwrap_or(0),
        n_ctx: v
            .get("default_generation_settings")
            .and_then(|x| x.get("n_ctx"))
            .and_then(|x| x.as_i64())
            .unwrap_or(0),
        build_info: v
            .get("build_info")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        is_sleeping: v
            .get("is_sleeping")
            .and_then(|x| x.as_bool())
            .unwrap_or(false),
    })
}

fn parse_gpu(text: &str) -> Vec<GpuInfo> {
    let mut gpus = vec![];
    for line in text.lines() {
        let parts: Vec<&str> = line.split(", ").collect();
        if parts.len() >= 10 {
            let parse_f = |s: &str| {
                s.trim_end_matches(" W")
                    .trim_end_matches(" %")
                    .trim_end_matches(" MiB")
                    .trim_end_matches(" MHz")
                    .parse::<f64>()
                    .unwrap_or(0.0)
            };
            let parse_i = |s: &str| s.trim_end_matches(" MHz").parse::<i64>().unwrap_or(0);
            let parse_u = |s: &str| s.trim_end_matches(" MiB").parse::<u64>().unwrap_or(0);
            gpus.push(GpuInfo {
                index: parse_f(parts[0].trim()) as i32,
                name: parts[1].trim().to_string(),
                gpu_util: parse_f(parts[2].trim().trim_end_matches(" %")),
                mem_used: parse_u(parts[3].trim()),
                mem_total: parse_u(parts[4].trim()),
                temp: parse_f(parts[5].trim().trim_end_matches(" C")),
                power_draw: parse_f(parts[6].trim()),
                power_limit: parse_f(parts[7].trim()),
                clock_gr: parse_i(parts[8].trim()),
                clock_mem: parse_i(parts[9].trim()),
                fan_speed: if parts.len() > 10 {
                    parse_f(parts[10].trim().trim_end_matches(" %"))
                } else {
                    0.0
                },
            });
        }
    }
    gpus
}

fn query_gpus() -> Vec<GpuInfo> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw,power.limit,clocks.gr,clocks.mem,fan.speed",
            "--format=csv,noheader,nounits",
        ])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            parse_gpu(&text)
        }
        _ => vec![],
    }
}

/// Collect every data source exactly once for one scheduler tick.
///
/// Timing belongs to `App`; this function deliberately has no per-component
/// cadence or cache timer.
pub fn fetch_snapshot(base_url: &str, prev: &Option<Snapshot>) -> Snapshot {
    let mut snap = Snapshot::new();
    let metrics_url = format!("{}/metrics", base_url);
    let slots_url = format!("{}/slots", base_url);
    let props_url = format!("{}/props", base_url);

    let mut connected = false;
    let mut errors = vec![];

    // Fetch metrics
    if let Some(text) = http_get(&metrics_url) {
        let response = text.trim_start();
        if !response.starts_with('{') && response.contains("llamacpp:") {
            // Prometheus format
            snap.metrics = parse_prometheus(response);
            snap.metrics_available = true;
            connected = true;
        } else if response.starts_with('{') {
            // Error response from server (model loading, etc.)
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(response) {
                if let Some(msg) = v
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                {
                    errors.push(msg.to_string());
                }
            }
        } else {
            errors.push("Invalid /metrics response".to_string());
        }
    } else {
        errors.push("Cannot reach /metrics".to_string());
    }

    // Fetch slots
    match http_get_result(&slots_url) {
        Ok(text) => match parse_slots(&text) {
            Ok(slots) => {
                snap.slots = slots;
                connected = true;
            }
            Err(error) => {
                let message = format!("invalid /slots response: {error}");
                snap.slots_error = Some(message.clone());
                errors.push(message);
            }
        },
        Err(error) => {
            let message = format!("cannot reach /slots: {error}");
            snap.slots_error = Some(message.clone());
            errors.push(message);
        }
    }

    // Fetch properties in the same collection cycle as every other source.
    // Keep the last valid value only when this cycle's request fails.
    if let Some(text) = http_get(&props_url) {
        snap.props = parse_props(&text);
    }
    if snap.props.is_none() {
        snap.props = prev.as_ref().and_then(|snapshot| snapshot.props.clone());
    }

    // Query GPUs
    snap.gpus = query_gpus();
    if !snap.gpus.is_empty() {
        connected = true;
    }

    // Carry over previous metrics for delta calculation
    snap.prev_metrics = prev
        .as_ref()
        .filter(|snapshot| snapshot.metrics_available)
        .map(|snapshot| snapshot.metrics.clone());
    snap.connected = connected;
    snap.error = if errors.is_empty() {
        None
    } else {
        Some(errors.join("; "))
    };

    snap
}

/// Auto-detect a llama.cpp server by scanning common ports
pub fn detect_server() -> Option<String> {
    // First try checking /proc for llama-server processes and extract --port
    if let Some(url) = detect_from_proc() {
        return Some(url);
    }

    // Fallback: scan common ports on localhost
    let ports = [8080, 8081, 8888, 5000, 3000, 9000, 18081, 18080, 11434];
    for &port in &ports {
        let url = format!("http://127.0.0.1:{}", port);
        if let Some(text) = http_get(&format!("{}/health", url)) {
            // Check if it looks like a llama.cpp server
            if text.contains("ok") || text.contains("Loading model") || text.contains("unavailable")
            {
                // Verify it's llama.cpp by checking /metrics or /props
                if let Some(metrics) = http_get(&format!("{}/metrics", url)) {
                    if metrics.contains("llamacpp:") {
                        return Some(url);
                    }
                }
            }
        }
    }
    None
}

fn detect_from_proc() -> Option<String> {
    // Read /proc/*/cmdline to find llama-server processes
    let proc_dir = match std::fs::read_dir("/proc") {
        Ok(d) => d,
        Err(_) => return None,
    };

    for entry in proc_dir.flatten() {
        let pid = entry.file_name();
        let pid_str = pid.to_string_lossy();
        if !pid_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }

        let cmdline_path = format!("/proc/{}/cmdline", pid_str);
        let cmdline = match std::fs::read_to_string(&cmdline_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // cmdline uses null bytes as separators
        let cmdline = cmdline.replace('\0', " ");

        // Check if this is a llama-server process
        if !cmdline.contains("llama-server") && !cmdline.contains("llama.cpp") {
            continue;
        }

        // Extract --port argument
        let port = extract_arg(&cmdline, "--port");
        let host = extract_arg(&cmdline, "--host").unwrap_or_else(|| "127.0.0.1".to_string());

        if let Some(p) = port {
            let host = if host == "0.0.0.0" {
                "127.0.0.1"
            } else {
                &host
            };
            let url = format!("http://{}:{}", host, p);
            // Quick verify
            if let Some(text) = http_get(&format!("{}/health", url)) {
                if text.contains("ok")
                    || text.contains("Loading model")
                    || text.contains("unavailable")
                {
                    return Some(url);
                }
            }
        }
    }

    None
}

fn extract_arg(cmdline: &str, flag: &str) -> Option<String> {
    let parts: Vec<&str> = cmdline.split_whitespace().collect();
    for i in 0..parts.len() {
        if parts[i] == flag && i + 1 < parts.len() {
            return Some(parts[i + 1].to_string());
        }
        // Also handle --flag=value
        if let Some(value) = parts[i]
            .strip_prefix(flag)
            .and_then(|value| value.strip_prefix('='))
        {
            return Some(value.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_prompt_metrics_with_standard_whitespace_and_optional_labels() {
        let metrics = parse_prometheus(
            r#"
                # TYPE llamacpp:prompt_tokens_total counter
                llamacpp:prompt_tokens_total 1300
                llamacpp:prompt_seconds_total      2.75
                llamacpp:prompt_tokens_seconds{model="test"} 472.7
            "#,
        );

        assert_eq!(metrics.prompt_tokens_total, 1_300.0);
        assert_eq!(metrics.prompt_seconds_total, 2.75);
        assert_eq!(metrics.prompt_tokens_seconds, 472.7);
    }

    #[test]
    fn parses_current_array_shaped_next_token_data() {
        let slots = parse_slots(
            r#"[{
                "id": 2,
                "id_task": 99,
                "n_ctx": 4096,
                "speculative": true,
                "is_processing": true,
                "n_prompt_tokens": 120,
                "n_prompt_tokens_processed": 30,
                "n_prompt_tokens_cache": 50,
                "next_token": [{"n_decoded": 17}]
            }]"#,
        )
        .unwrap();

        assert_eq!(slots.len(), 1);
        let slot = &slots[0];
        assert_eq!(slot.id, 2);
        assert_eq!(slot.task_id, Some(99));
        assert_eq!(slot.context_capacity, 4096);
        assert_eq!(slot.context_tokens, 120);
        assert_eq!(slot.prompt_tokens_processed, 30);
        assert_eq!(slot.prompt_tokens_cached, 50);
        assert_eq!(slot.decoded_tokens, 17);
        assert_eq!(slot.current_output_tokens(), 40);
    }

    #[test]
    fn parses_object_shaped_next_token_data_from_other_server_versions() {
        let slots = parse_slots(r#"[{"next_token":{"n_decoded":9}}]"#).unwrap();

        assert_eq!(slots[0].decoded_tokens, 9);
    }

    #[test]
    fn rejects_non_array_slot_payloads_instead_of_looking_empty() {
        let error = parse_slots(r#"{"error":"slots disabled"}"#).unwrap_err();

        assert!(error.contains("expected a JSON array"));
    }

    #[test]
    fn idle_slots_do_not_expose_stale_output_as_current() {
        let slot = SlotInfo {
            is_processing: false,
            context_tokens: 120,
            prompt_tokens_processed: 30,
            prompt_tokens_cached: 50,
            decoded_tokens: 17,
            ..SlotInfo::default()
        };

        assert_eq!(slot.current_output_tokens(), 0);
    }
}
