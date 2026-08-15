#![allow(dead_code)]

use std::time::Instant;

#[derive(Debug, Clone, Default)]
pub struct RequestParams {
    pub max_tokens: Option<i64>,
    pub temperature: Option<f64>,
    pub top_k: Option<i64>,
    pub top_p: Option<f64>,
    pub min_p: Option<f64>,
    pub stream: Option<bool>,
    pub chat_format: String,
    pub reasoning_format: String,
    pub speculative_types: String,
}

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
    pub remaining_tokens: Option<i64>,
    pub has_next_token: Option<bool>,
    pub params: RequestParams,
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

    pub fn phase(&self) -> &'static str {
        if !self.is_processing {
            "idle"
        } else if self.current_output_tokens() > 0 {
            "decode"
        } else {
            "prefill"
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Modalities {
    pub vision: bool,
    pub video: bool,
    pub audio: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ChatCapabilities {
    pub tools: bool,
    pub parallel_tool_calls: bool,
    pub system_role: bool,
    pub typed_content: bool,
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
    pub endpoint_slots: Option<bool>,
    pub endpoint_metrics: Option<bool>,
    pub ui_enabled: Option<bool>,
    pub cors_proxy_enabled: Option<bool>,
    pub modalities: Modalities,
    pub chat_capabilities: ChatCapabilities,
    pub default_generation: RequestParams,
}

#[derive(Debug, Clone, Default)]
pub struct ModelInfo {
    pub id: String,
    pub format: String,
    pub parameter_count: u64,
    pub size_bytes: u64,
    pub context_size: i64,
    pub trained_context_size: i64,
    pub embedding_size: i64,
    pub vocabulary_size: i64,
    pub ftype: String,
}

#[derive(Debug, Clone, Default)]
pub struct HostInfo {
    pub memory_total_kib: u64,
    pub memory_available_kib: u64,
    pub swap_total_kib: u64,
    pub swap_free_kib: u64,
    pub load_one: f64,
    pub load_five: f64,
    pub load_fifteen: f64,
    pub logical_cpus: usize,
}

#[derive(Debug, Clone, Default)]
pub struct LocalServerInfo {
    pub pid: u32,
    pub binary_path: String,
    pub bind_host: String,
    pub port: u16,
    pub process_uptime_seconds: Option<u64>,
    pub rss_kib: Option<u64>,
    pub threads: Option<u32>,
    pub cgroup_memory_current: Option<u64>,
    pub cgroup_memory_limit: Option<u64>,
    pub cgroup_swap_limit: Option<u64>,
    pub draft_model: String,
    pub devices: String,
    pub split_mode: String,
    pub parallel: Option<i64>,
    pub speculative_type: String,
    pub speculative_max_tokens: Option<i64>,
    pub batch_size: Option<i64>,
    pub ubatch_size: Option<i64>,
    pub cache_ram_mib: Option<i64>,
    pub cache_type_k: String,
    pub cache_type_v: String,
    pub flash_attention: Option<bool>,
    pub web_ui_enabled: Option<bool>,
    pub api_key_configured: bool,
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
    pub model: Option<ModelInfo>,
    pub local_server: Option<LocalServerInfo>,
    pub host: Option<HostInfo>,
    pub gpus: Vec<GpuInfo>,
    pub last_request_timings: Option<RequestTimings>,
    pub connected: bool,
    pub error: Option<String>,
    pub metrics_error: Option<String>,
    pub slots_error: Option<String>,
    pub props_error: Option<String>,
    pub model_error: Option<String>,
    pub gpu_error: Option<String>,
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

fn optional_i64(value: Option<&serde_json::Value>) -> Option<i64> {
    value.and_then(serde_json::Value::as_i64)
}

fn optional_f64(value: Option<&serde_json::Value>) -> Option<f64> {
    value.and_then(serde_json::Value::as_f64)
}

fn optional_bool(value: Option<&serde_json::Value>) -> Option<bool> {
    value.and_then(serde_json::Value::as_bool)
}

fn parse_request_params(value: Option<&serde_json::Value>) -> RequestParams {
    let Some(value) = value else {
        return RequestParams::default();
    };

    RequestParams {
        max_tokens: optional_i64(value.get("max_tokens"))
            .filter(|value| *value >= 0)
            .or_else(|| optional_i64(value.get("n_predict")).filter(|value| *value >= 0)),
        temperature: optional_f64(value.get("temperature")),
        top_k: optional_i64(value.get("top_k")),
        top_p: optional_f64(value.get("top_p")),
        min_p: optional_f64(value.get("min_p")),
        stream: optional_bool(value.get("stream")),
        chat_format: value
            .get("chat_format")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string(),
        reasoning_format: value
            .get("reasoning_format")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string(),
        speculative_types: value
            .get("speculative.types")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string(),
    }
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
            let next_token = match next_token {
                Some(serde_json::Value::Array(tokens)) => tokens.first(),
                Some(serde_json::Value::Object(_)) => next_token,
                _ => None,
            };
            let decoded_tokens = next_token
                .and_then(|token| token.get("n_decoded"))
                .and_then(serde_json::Value::as_i64)
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
                remaining_tokens: optional_i64(next_token.and_then(|token| token.get("n_remain"))),
                has_next_token: optional_bool(
                    next_token.and_then(|token| token.get("has_next_token")),
                ),
                params: parse_request_params(slot.get("params")),
            }
        })
        .collect())
}

fn parse_props(json_text: &str) -> Option<ServerProps> {
    let v: serde_json::Value = serde_json::from_str(json_text).ok()?;
    let modalities = v.get("modalities");
    let chat_capabilities = v.get("chat_template_caps");
    let default_params = v
        .get("default_generation_settings")
        .and_then(|settings| settings.get("params"));
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
        endpoint_slots: optional_bool(v.get("endpoint_slots")),
        endpoint_metrics: optional_bool(v.get("endpoint_metrics")),
        ui_enabled: optional_bool(v.get("ui")),
        cors_proxy_enabled: optional_bool(v.get("cors_proxy_enabled")),
        modalities: Modalities {
            vision: optional_bool(modalities.and_then(|value| value.get("vision")))
                .unwrap_or(false),
            video: optional_bool(modalities.and_then(|value| value.get("video"))).unwrap_or(false),
            audio: optional_bool(modalities.and_then(|value| value.get("audio"))).unwrap_or(false),
        },
        chat_capabilities: ChatCapabilities {
            tools: optional_bool(chat_capabilities.and_then(|value| value.get("supports_tools")))
                .unwrap_or(false),
            parallel_tool_calls: optional_bool(
                chat_capabilities.and_then(|value| value.get("supports_parallel_tool_calls")),
            )
            .unwrap_or(false),
            system_role: optional_bool(
                chat_capabilities.and_then(|value| value.get("supports_system_role")),
            )
            .unwrap_or(false),
            typed_content: optional_bool(
                chat_capabilities.and_then(|value| value.get("supports_typed_content")),
            )
            .unwrap_or(false),
        },
        default_generation: parse_request_params(default_params),
    })
}

fn parse_models(json_text: &str) -> Result<ModelInfo, String> {
    let value: serde_json::Value =
        serde_json::from_str(json_text).map_err(|error| format!("invalid JSON: {error}"))?;
    let model = value
        .get("data")
        .and_then(serde_json::Value::as_array)
        .and_then(|models| models.first())
        .ok_or_else(|| "expected a model in data[]".to_string())?;
    let meta = model.get("meta");

    Ok(ModelInfo {
        id: model
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string(),
        format: meta
            .and_then(|value| value.get("format"))
            .and_then(serde_json::Value::as_str)
            .or_else(|| {
                value
                    .get("models")
                    .and_then(serde_json::Value::as_array)
                    .and_then(|models| models.first())
                    .and_then(|model| model.get("details"))
                    .and_then(|details| details.get("format"))
                    .and_then(serde_json::Value::as_str)
            })
            .unwrap_or("")
            .to_string(),
        parameter_count: meta
            .and_then(|value| value.get("n_params"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        size_bytes: meta
            .and_then(|value| value.get("size"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0),
        context_size: meta
            .and_then(|value| value.get("n_ctx"))
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0),
        trained_context_size: meta
            .and_then(|value| value.get("n_ctx_train"))
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0),
        embedding_size: meta
            .and_then(|value| value.get("n_embd"))
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0),
        vocabulary_size: meta
            .and_then(|value| value.get("n_vocab"))
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0),
        ftype: meta
            .and_then(|value| value.get("ftype"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string(),
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

fn query_gpus() -> Result<Vec<GpuInfo>, String> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw,power.limit,clocks.gr,clocks.mem,fan.speed",
            "--format=csv,noheader,nounits",
        ])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            let gpus = parse_gpu(&text);
            if gpus.is_empty() {
                Err("nvidia-smi returned no parseable devices".to_string())
            } else {
                Ok(gpus)
            }
        }
        Ok(output) => Err(format!(
            "nvidia-smi exited with {}",
            output
                .status
                .code()
                .map_or_else(|| "a signal".to_string(), |code| code.to_string())
        )),
        Err(error) => Err(format!("cannot run nvidia-smi: {error}")),
    }
}

fn local_url_port(base_url: &str) -> Option<u16> {
    let (scheme, remainder) = base_url.split_once("://")?;
    let authority = remainder.split('/').next()?.rsplit('@').next()?;
    let (host, port) = if let Some(authority) = authority.strip_prefix('[') {
        let (host, suffix) = authority.split_once(']')?;
        let port = suffix.strip_prefix(':').and_then(|port| port.parse().ok());
        (host, port)
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        (host, port.parse().ok())
    } else {
        (authority, None)
    };

    let is_local = host.eq_ignore_ascii_case("localhost")
        || host == "::1"
        || host == "0.0.0.0"
        || host.starts_with("127.");
    if !is_local {
        return None;
    }

    port.or(match scheme {
        "http" => Some(80),
        "https" => Some(443),
        _ => None,
    })
}

fn process_args(pid: u32) -> Option<Vec<String>> {
    let bytes = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let args: Vec<String> = bytes
        .split(|byte| *byte == 0)
        .filter(|argument| !argument.is_empty())
        .map(|argument| String::from_utf8_lossy(argument).into_owned())
        .collect();
    (!args.is_empty()).then_some(args)
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter().enumerate().find_map(|(index, argument)| {
        if argument == flag {
            args.get(index + 1).cloned()
        } else {
            argument
                .strip_prefix(flag)
                .and_then(|value| value.strip_prefix('='))
                .map(str::to_string)
        }
    })
}

fn arg_i64(args: &[String], flag: &str) -> Option<i64> {
    arg_value(args, flag).and_then(|value| value.parse().ok())
}

fn has_arg(args: &[String], flag: &str) -> bool {
    args.iter()
        .any(|argument| argument == flag || argument.starts_with(&format!("{flag}=")))
}

fn bool_arg(args: &[String], flag: &str, negative_flag: &str) -> Option<bool> {
    if has_arg(args, negative_flag) {
        return Some(false);
    }
    arg_value(args, flag)
        .map(|value| !matches!(value.as_str(), "0" | "false" | "off" | "no"))
        .or_else(|| has_arg(args, flag).then_some(true))
}

fn is_llama_server(args: &[String]) -> bool {
    args.first()
        .and_then(|argument| std::path::Path::new(argument).file_name())
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.contains("llama-server"))
}

fn process_matches_port(pid: u32, port: u16) -> Option<Vec<String>> {
    let args = process_args(pid)?;
    if !is_llama_server(&args) {
        return None;
    }
    let process_port = arg_value(&args, "--port")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8080);
    (process_port == port).then_some(args)
}

fn find_server_process(port: u16, previous_pid: Option<u32>) -> Option<(u32, Vec<String>)> {
    if let Some(pid) = previous_pid {
        if let Some(args) = process_matches_port(pid, port) {
            return Some((pid, args));
        }
    }

    let entries = std::fs::read_dir("/proc").ok()?;
    entries.flatten().find_map(|entry| {
        let pid = entry.file_name().to_string_lossy().parse::<u32>().ok()?;
        process_matches_port(pid, port).map(|args| (pid, args))
    })
}

#[cfg(target_os = "linux")]
fn process_uptime_seconds(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields: Vec<&str> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    // The first field after the command name is field 3 (`state`); process
    // start time is field 22, hence index 19 in this tail.
    let start_ticks = fields.get(19)?.parse::<f64>().ok()?;
    let system_uptime = std::fs::read_to_string("/proc/uptime")
        .ok()?
        .split_whitespace()
        .next()?
        .parse::<f64>()
        .ok()?;
    let ticks_per_second = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if ticks_per_second <= 0 {
        return None;
    }
    Some(
        (system_uptime - start_ticks / ticks_per_second as f64)
            .max(0.0)
            .round() as u64,
    )
}

#[cfg(not(target_os = "linux"))]
fn process_uptime_seconds(_pid: u32) -> Option<u64> {
    None
}

fn process_status(pid: u32) -> (Option<u64>, Option<u32>) {
    let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) else {
        return (None, None);
    };
    let mut rss_kib = None;
    let mut threads = None;
    for line in status.lines() {
        if let Some(value) = line.strip_prefix("VmRSS:") {
            rss_kib = value
                .split_whitespace()
                .next()
                .and_then(|value| value.parse().ok());
        } else if let Some(value) = line.strip_prefix("Threads:") {
            threads = value.trim().parse().ok();
        }
    }
    (rss_kib, threads)
}

fn read_u64_or_max(path: &std::path::Path) -> Option<u64> {
    let value = std::fs::read_to_string(path).ok()?;
    let value = value.trim();
    (value != "max").then(|| value.parse().ok()).flatten()
}

fn cgroup_memory(pid: u32) -> (Option<u64>, Option<u64>, Option<u64>) {
    let Ok(cgroups) = std::fs::read_to_string(format!("/proc/{pid}/cgroup")) else {
        return (None, None, None);
    };
    let Some(path) = cgroups.lines().find_map(|line| line.strip_prefix("0::")) else {
        return (None, None, None);
    };
    let root = std::path::Path::new("/sys/fs/cgroup").join(path.trim_start_matches('/'));
    (
        read_u64_or_max(&root.join("memory.current")),
        read_u64_or_max(&root.join("memory.max")),
        read_u64_or_max(&root.join("memory.swap.max")),
    )
}

fn basename(path: Option<String>) -> String {
    path.as_deref()
        .and_then(|path| std::path::Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_string()
}

fn query_local_server(
    base_url: &str,
    previous: Option<&LocalServerInfo>,
) -> Option<LocalServerInfo> {
    let port = local_url_port(base_url)?;
    let (pid, args) = find_server_process(port, previous.map(|server| server.pid))?;
    let (rss_kib, threads) = process_status(pid);
    let (cgroup_memory_current, cgroup_memory_limit, cgroup_swap_limit) = cgroup_memory(pid);

    Some(LocalServerInfo {
        pid,
        binary_path: args.first().cloned().unwrap_or_default(),
        bind_host: arg_value(&args, "--host").unwrap_or_else(|| "127.0.0.1".to_string()),
        port,
        process_uptime_seconds: process_uptime_seconds(pid),
        rss_kib,
        threads,
        cgroup_memory_current,
        cgroup_memory_limit,
        cgroup_swap_limit,
        draft_model: basename(arg_value(&args, "--model-draft")),
        devices: arg_value(&args, "--device").unwrap_or_default(),
        split_mode: arg_value(&args, "--split-mode").unwrap_or_default(),
        parallel: arg_i64(&args, "--parallel"),
        speculative_type: arg_value(&args, "--spec-type").unwrap_or_default(),
        speculative_max_tokens: arg_i64(&args, "--spec-draft-n-max"),
        batch_size: arg_i64(&args, "--batch-size"),
        ubatch_size: arg_i64(&args, "--ubatch-size"),
        cache_ram_mib: arg_i64(&args, "--cache-ram"),
        cache_type_k: arg_value(&args, "--cache-type-k").unwrap_or_default(),
        cache_type_v: arg_value(&args, "--cache-type-v").unwrap_or_default(),
        flash_attention: bool_arg(&args, "--flash-attn", "--no-flash-attn"),
        web_ui_enabled: Some(!has_arg(&args, "--no-webui")),
        api_key_configured: has_arg(&args, "--api-key") || has_arg(&args, "--api-key-file"),
    })
}

fn query_host() -> Option<HostInfo> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let mut host = HostInfo::default();
    for line in meminfo.lines() {
        let mut fields = line.split_whitespace();
        let key = fields.next()?.trim_end_matches(':');
        let value = fields.next().and_then(|value| value.parse::<u64>().ok());
        match (key, value) {
            ("MemTotal", Some(value)) => host.memory_total_kib = value,
            ("MemAvailable", Some(value)) => host.memory_available_kib = value,
            ("SwapTotal", Some(value)) => host.swap_total_kib = value,
            ("SwapFree", Some(value)) => host.swap_free_kib = value,
            _ => {}
        }
    }

    let load = std::fs::read_to_string("/proc/loadavg").ok()?;
    let mut load = load.split_whitespace();
    host.load_one = load.next()?.parse().ok()?;
    host.load_five = load.next()?.parse().ok()?;
    host.load_fifteen = load.next()?.parse().ok()?;
    host.logical_cpus = std::thread::available_parallelism().map_or(0, usize::from);
    Some(host)
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
    let models_url = format!("{}/v1/models", base_url);

    let mut connected = false;
    let mut errors = vec![];

    // Fetch metrics
    match http_get_result(&metrics_url) {
        Ok(text) => {
            let response = text.trim_start();
            if !response.starts_with('{') && response.contains("llamacpp:") {
                // Prometheus format
                snap.metrics = parse_prometheus(response);
                snap.metrics_available = true;
                connected = true;
            } else if response.starts_with('{') {
                // Error response from server (model loading, etc.)
                let message = serde_json::from_str::<serde_json::Value>(response)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("error")
                            .and_then(|error| error.get("message"))
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string)
                    })
                    .unwrap_or_else(|| "invalid /metrics response".to_string());
                snap.metrics_error = Some(message.clone());
                errors.push(message);
            } else {
                let message = "invalid /metrics response".to_string();
                snap.metrics_error = Some(message.clone());
                errors.push(message);
            }
        }
        Err(error) => {
            let message = format!("cannot reach /metrics: {error}");
            snap.metrics_error = Some(message.clone());
            errors.push(message);
        }
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
    match http_get_result(&props_url) {
        Ok(text) => {
            snap.props = parse_props(&text);
            if snap.props.is_some() {
                connected = true;
            } else {
                let message = "invalid /props response".to_string();
                snap.props_error = Some(message.clone());
                errors.push(message);
            }
        }
        Err(error) => {
            let message = format!("cannot reach /props: {error}");
            snap.props_error = Some(message.clone());
            errors.push(message);
        }
    }
    if snap.props.is_none() {
        snap.props = prev.as_ref().and_then(|snapshot| snapshot.props.clone());
    }

    // Model metadata is richer than /props (parameter count, on-disk size,
    // trained context). It is optional for compatibility with older servers;
    // retain the last valid value and expose failures in source health without
    // marking otherwise usable telemetry as degraded.
    match http_get_result(&models_url) {
        Ok(text) => match parse_models(&text) {
            Ok(model) => {
                snap.model = Some(model);
                connected = true;
            }
            Err(error) => snap.model_error = Some(format!("invalid /v1/models response: {error}")),
        },
        Err(error) => snap.model_error = Some(format!("cannot reach /v1/models: {error}")),
    }
    if snap.model.is_none() {
        snap.model = prev.as_ref().and_then(|snapshot| snapshot.model.clone());
    }

    snap.local_server = query_local_server(
        base_url,
        prev.as_ref()
            .and_then(|snapshot| snapshot.local_server.as_ref()),
    );
    if snap.local_server.is_some() {
        snap.host = query_host();
    }

    // nvidia-smi always describes the monitor host. Only associate it with the
    // server when the URL is local; showing client GPUs for a remote URL would
    // be actively misleading.
    if snap.local_server.is_some() {
        match query_gpus() {
            Ok(gpus) => snap.gpus = gpus,
            Err(error) => snap.gpu_error = Some(error),
        }
    } else if local_url_port(base_url).is_some() {
        snap.gpu_error = Some(
            "URL is local but does not match a local llama-server; refusing to attribute monitor GPUs"
                .to_string(),
        );
    } else {
        snap.gpu_error =
            Some("remote GPU telemetry is unavailable from local nvidia-smi".to_string());
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
    let proc_dir = std::fs::read_dir("/proc").ok()?;
    for entry in proc_dir.flatten() {
        let Some(pid) = entry.file_name().to_string_lossy().parse::<u32>().ok() else {
            continue;
        };
        let Some(args) = process_args(pid).filter(|args| is_llama_server(args)) else {
            continue;
        };
        let port = arg_value(&args, "--port").unwrap_or_else(|| "8080".to_string());
        let host = arg_value(&args, "--host").unwrap_or_else(|| "127.0.0.1".to_string());
        let connect_host = if matches!(host.as_str(), "0.0.0.0" | "::") {
            "127.0.0.1"
        } else {
            host.as_str()
        };
        let url = format!("http://{connect_host}:{port}");
        if let Some(text) = http_get(&format!("{url}/health")) {
            if text.contains("ok") || text.contains("Loading model") || text.contains("unavailable")
            {
                return Some(url);
            }
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
                "params": {
                    "max_tokens": 512,
                    "temperature": 0.7,
                    "top_k": 40,
                    "top_p": 0.95,
                    "min_p": 0.05,
                    "stream": true,
                    "chat_format": "peg-native",
                    "reasoning_format": "deepseek",
                    "speculative.types": "none,draft-dspark"
                },
                "next_token": [{
                    "n_decoded": 17,
                    "n_remain": 495,
                    "has_next_token": true
                }]
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
        assert_eq!(slot.remaining_tokens, Some(495));
        assert_eq!(slot.has_next_token, Some(true));
        assert_eq!(slot.params.max_tokens, Some(512));
        assert_eq!(slot.params.temperature, Some(0.7));
        assert_eq!(slot.params.chat_format, "peg-native");
        assert_eq!(slot.params.reasoning_format, "deepseek");
        assert_eq!(slot.params.speculative_types, "none,draft-dspark");
        assert_eq!(slot.current_output_tokens(), 40);
        assert_eq!(slot.phase(), "decode");
    }

    #[test]
    fn parses_rich_server_properties_without_requiring_every_capability() {
        let props = parse_props(
            r#"{
                "model_alias": "deepseek-v4",
                "model_path": "/models/deepseek.gguf",
                "model_ftype": "IQ4_XS",
                "total_slots": 1,
                "default_generation_settings": {
                    "n_ctx": 262144,
                    "params": {
                        "max_tokens": -1,
                        "n_predict": -1,
                        "temperature": 1.0,
                        "top_k": 40,
                        "stream": false
                    }
                },
                "endpoint_slots": true,
                "endpoint_metrics": true,
                "ui": false,
                "cors_proxy_enabled": false,
                "modalities": {"vision": false, "video": false, "audio": false},
                "chat_template_caps": {
                    "supports_tools": true,
                    "supports_parallel_tool_calls": true,
                    "supports_system_role": true
                },
                "build_info": "b10270",
                "is_sleeping": false
            }"#,
        )
        .unwrap();

        assert_eq!(props.model_alias, "deepseek-v4");
        assert_eq!(props.n_ctx, 262_144);
        assert_eq!(props.endpoint_slots, Some(true));
        assert_eq!(props.ui_enabled, Some(false));
        assert!(props.chat_capabilities.tools);
        assert!(props.chat_capabilities.parallel_tool_calls);
        assert_eq!(props.default_generation.temperature, Some(1.0));
        assert_eq!(props.default_generation.top_k, Some(40));
        assert_eq!(props.default_generation.max_tokens, None);
    }

    #[test]
    fn parses_openai_model_metadata_for_scale_and_trained_context() {
        let model = parse_models(
            r#"{
                "models": [{"details": {"format": "gguf"}}],
                "data": [{
                    "id": "deepseek-v4",
                    "meta": {
                        "n_vocab": 129280,
                        "n_ctx": 262144,
                        "n_ctx_train": 1048576,
                        "n_embd": 4096,
                        "n_params": 284334567511,
                        "size": 136657101148,
                        "ftype": "IQ4_XS - 4.25 bpw"
                    }
                }]
            }"#,
        )
        .unwrap();

        assert_eq!(model.id, "deepseek-v4");
        assert_eq!(model.format, "gguf");
        assert_eq!(model.parameter_count, 284_334_567_511);
        assert_eq!(model.size_bytes, 136_657_101_148);
        assert_eq!(model.context_size, 262_144);
        assert_eq!(model.trained_context_size, 1_048_576);
        assert_eq!(model.embedding_size, 4096);
        assert_eq!(model.vocabulary_size, 129_280);
    }

    #[test]
    fn local_url_detection_does_not_attribute_monitor_gpus_to_remote_servers() {
        assert_eq!(local_url_port("http://127.0.0.1:18081"), Some(18_081));
        assert_eq!(local_url_port("http://localhost"), Some(80));
        assert_eq!(local_url_port("https://[::1]:9443/api"), Some(9443));
        assert_eq!(local_url_port("http://inference-host:8080"), None);
        assert_eq!(local_url_port("http://192.168.1.50:8080"), None);
    }

    #[test]
    fn process_argument_parsing_handles_split_and_equals_forms() {
        let args = vec![
            "/opt/llama-server".to_string(),
            "--port=18081".to_string(),
            "--host".to_string(),
            "0.0.0.0".to_string(),
            "--no-webui".to_string(),
        ];

        assert!(is_llama_server(&args));
        assert_eq!(arg_value(&args, "--port").as_deref(), Some("18081"));
        assert_eq!(arg_value(&args, "--host").as_deref(), Some("0.0.0.0"));
        assert!(has_arg(&args, "--no-webui"));
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

    #[test]
    fn new_prefill_ignores_the_previous_tasks_decoded_counter() {
        let slot = SlotInfo {
            is_processing: true,
            context_tokens: 120,
            prompt_tokens_processed: 70,
            prompt_tokens_cached: 50,
            // llama.cpp resets this only when the new prompt finishes.
            decoded_tokens: 17,
            ..SlotInfo::default()
        };

        assert_eq!(slot.current_output_tokens(), 0);
        assert_eq!(slot.phase(), "prefill");
    }
}
