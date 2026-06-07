//! Built-in neutral tools (file / shell / code / network).

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Platform, Tool};
use crate::agent::tool::{ToolContext, ToolResult};

/// Resolve a (possibly relative) path against the workspace root, rejecting
/// traversal that escapes the root.
fn resolve(ctx: &ToolContext, path: &str) -> Result<PathBuf, String> {
    let candidate = Path::new(path);
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        ctx.workspace_root.join(candidate)
    };
    let normalized = normalize(&joined);
    let root = normalize(&ctx.workspace_root);
    if ctx.bypass_permissions || normalized.starts_with(&root) {
        Ok(normalized)
    } else {
        Err(format!("path '{path}' is outside the workspace"))
    }
}

fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn str_arg<'a>(input: &'a Value, key: &str) -> Result<&'a str, String> {
    input
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("missing required string argument '{key}'"))
}

// ── file_read ────────────────────────────────────────────────────────────────

pub struct FileReadTool;

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "file_read"
    }
    fn description(&self) -> &str {
        "Read a UTF-8 text file from the workspace."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "path": { "type": "string", "description": "Path relative to the workspace root." } },
            "required": ["path"]
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolResult {
        let path = match str_arg(&input, "path") {
            Ok(p) => p,
            Err(e) => return ToolResult::error(e),
        };
        match resolve(ctx, path) {
            Ok(p) => match std::fs::read_to_string(&p) {
                Ok(content) => ToolResult::ok(content),
                Err(e) => ToolResult::error(format!("read failed: {e}")),
            },
            Err(e) => ToolResult::error(e),
        }
    }
}

// ── file_write ───────────────────────────────────────────────────────────────

pub struct FileWriteTool;

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "file_write"
    }
    fn description(&self) -> &str {
        "Create or overwrite a file in the workspace with the given content."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["path", "content"]
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolResult {
        let path = match str_arg(&input, "path") {
            Ok(p) => p,
            Err(e) => return ToolResult::error(e),
        };
        let content = input.get("content").and_then(|v| v.as_str()).unwrap_or("");
        match resolve(ctx, path) {
            Ok(p) => {
                if let Some(parent) = p.parent() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        return ToolResult::error(format!("mkdir failed: {e}"));
                    }
                }
                match std::fs::write(&p, content) {
                    Ok(()) => ToolResult::ok(format!("wrote {} bytes to {path}", content.len())),
                    Err(e) => ToolResult::error(format!("write failed: {e}")),
                }
            }
            Err(e) => ToolResult::error(e),
        }
    }
}

// ── file_edit ────────────────────────────────────────────────────────────────

pub struct FileEditTool;

#[async_trait]
impl Tool for FileEditTool {
    fn name(&self) -> &str {
        "file_edit"
    }
    fn description(&self) -> &str {
        "Replace the first occurrence of `search` with `replace` in a file."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "search": { "type": "string" },
                "replace": { "type": "string" }
            },
            "required": ["path", "search", "replace"]
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolResult {
        let path = match str_arg(&input, "path") {
            Ok(p) => p,
            Err(e) => return ToolResult::error(e),
        };
        let search = match str_arg(&input, "search") {
            Ok(s) => s,
            Err(e) => return ToolResult::error(e),
        };
        let replace = input.get("replace").and_then(|v| v.as_str()).unwrap_or("");
        match resolve(ctx, path) {
            Ok(p) => match std::fs::read_to_string(&p) {
                Ok(content) => {
                    if !content.contains(search) {
                        return ToolResult::error("search string not found".to_string());
                    }
                    let updated = content.replacen(search, replace, 1);
                    match std::fs::write(&p, updated) {
                        Ok(()) => ToolResult::ok(format!("edited {path}")),
                        Err(e) => ToolResult::error(format!("write failed: {e}")),
                    }
                }
                Err(e) => ToolResult::error(format!("read failed: {e}")),
            },
            Err(e) => ToolResult::error(e),
        }
    }
}

// ── file_diff ────────────────────────────────────────────────────────────────

pub struct FileDiffTool;

#[async_trait]
impl Tool for FileDiffTool {
    fn name(&self) -> &str {
        "file_diff"
    }
    fn description(&self) -> &str {
        "Show a line-level diff between a file's current content and proposed content."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string", "description": "Proposed new content." }
            },
            "required": ["path", "content"]
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolResult {
        let path = match str_arg(&input, "path") {
            Ok(p) => p,
            Err(e) => return ToolResult::error(e),
        };
        let proposed = input.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let current = resolve(ctx, path)
            .ok()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        ToolResult::ok(line_diff(&current, proposed))
    }
}

/// A minimal line-by-line diff (no LCS — flags changed/added/removed lines).
fn line_diff(old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let max = old_lines.len().max(new_lines.len());
    let mut out = String::new();
    for i in 0..max {
        match (old_lines.get(i), new_lines.get(i)) {
            (Some(o), Some(n)) if o == n => out.push_str(&format!("  {o}\n")),
            (Some(o), Some(n)) => {
                out.push_str(&format!("- {o}\n+ {n}\n"));
            }
            (Some(o), None) => out.push_str(&format!("- {o}\n")),
            (None, Some(n)) => out.push_str(&format!("+ {n}\n")),
            (None, None) => {}
        }
    }
    if out.is_empty() {
        out.push_str("(no changes)");
    }
    out
}

// ── file_list ────────────────────────────────────────────────────────────────

pub struct FileListTool;

#[async_trait]
impl Tool for FileListTool {
    fn name(&self) -> &str {
        "file_list"
    }
    fn description(&self) -> &str {
        "List the entries of a directory in the workspace."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "path": { "type": "string", "description": "Directory path (default: workspace root)." } }
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolResult {
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        match resolve(ctx, path) {
            Ok(p) => match std::fs::read_dir(&p) {
                Ok(rd) => {
                    let mut entries: Vec<String> = rd
                        .filter_map(|e| e.ok())
                        .map(|e| {
                            let name = e.file_name().to_string_lossy().into_owned();
                            if e.path().is_dir() {
                                format!("{name}/")
                            } else {
                                name
                            }
                        })
                        .collect();
                    entries.sort();
                    ToolResult::ok(entries.join("\n"))
                }
                Err(e) => ToolResult::error(format!("readdir failed: {e}")),
            },
            Err(e) => ToolResult::error(e),
        }
    }
}

// ── file_search ──────────────────────────────────────────────────────────────

pub struct FileSearchTool;

#[async_trait]
impl Tool for FileSearchTool {
    fn name(&self) -> &str {
        "file_search"
    }
    fn description(&self) -> &str {
        "Recursively search workspace files for a substring; returns matching path:line."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "path": { "type": "string", "description": "Subtree to search (default: workspace root)." }
            },
            "required": ["query"]
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolResult {
        let query = match str_arg(&input, "query") {
            Ok(q) => q,
            Err(e) => return ToolResult::error(e),
        };
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let root = match resolve(ctx, path) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(e),
        };
        let mut hits: Vec<String> = Vec::new();
        search_dir(&root, query, &mut hits, 0);
        if hits.is_empty() {
            ToolResult::ok("(no matches)".to_string())
        } else {
            hits.truncate(200);
            ToolResult::ok(hits.join("\n"))
        }
    }
}

fn search_dir(dir: &Path, query: &str, hits: &mut Vec<String>, depth: usize) {
    if depth > 12 || hits.len() >= 200 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.filter_map(|e| e.ok()) {
        let p = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        if p.is_dir() {
            search_dir(&p, query, hits, depth + 1);
        } else if let Ok(content) = std::fs::read_to_string(&p) {
            for (i, line) in content.lines().enumerate() {
                if line.contains(query) {
                    hits.push(format!("{}:{}: {}", p.display(), i + 1, line.trim()));
                    if hits.len() >= 200 {
                        return;
                    }
                }
            }
        }
    }
}

// ── shell ────────────────────────────────────────────────────────────────────

pub struct ShellTool;

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }
    fn description(&self) -> &str {
        "Run a shell command in the workspace and return combined stdout/stderr."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "command": { "type": "string" } },
            "required": ["command"]
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolResult {
        let command = match str_arg(&input, "command") {
            Ok(c) => c.to_string(),
            Err(e) => return ToolResult::error(e),
        };
        run_shell(&command, &ctx.workspace_root).await
    }
}

async fn run_shell(command: &str, cwd: &Path) -> ToolResult {
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(command);
        c
    };
    #[cfg(not(target_os = "windows"))]
    let mut cmd = {
        let mut c = tokio::process::Command::new("sh");
        c.arg("-c").arg(command);
        c
    };
    cmd.current_dir(cwd);
    match cmd.output().await {
        Ok(out) => {
            let mut combined = String::new();
            combined.push_str(&String::from_utf8_lossy(&out.stdout));
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !stderr.is_empty() {
                combined.push_str("\n[stderr]\n");
                combined.push_str(&stderr);
            }
            if out.status.success() {
                ToolResult::ok(combined)
            } else {
                ToolResult::error(format!(
                    "exit code {}: {combined}",
                    out.status.code().unwrap_or(-1)
                ))
            }
        }
        Err(e) => ToolResult::error(format!("failed to spawn command: {e}")),
    }
}

// ── code_run ─────────────────────────────────────────────────────────────────

pub struct CodeRunTool;

#[async_trait]
impl Tool for CodeRunTool {
    fn name(&self) -> &str {
        "code_run"
    }
    fn description(&self) -> &str {
        "Execute a code snippet. `language` is python or bash (default bash)."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "language": { "type": "string", "enum": ["python", "bash"] },
                "code": { "type": "string" }
            },
            "required": ["code"]
        })
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolResult {
        let code = match str_arg(&input, "code") {
            Ok(c) => c.to_string(),
            Err(e) => return ToolResult::error(e),
        };
        let language = input
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("bash");
        match language {
            "python" => {
                let mut cmd = tokio::process::Command::new("python3");
                cmd.arg("-c").arg(&code).current_dir(&ctx.workspace_root);
                spawn_output(cmd).await
            }
            _ => run_shell(&code, &ctx.workspace_root).await,
        }
    }
}

async fn spawn_output(mut cmd: tokio::process::Command) -> ToolResult {
    match cmd.output().await {
        Ok(out) => {
            let mut combined = String::new();
            combined.push_str(&String::from_utf8_lossy(&out.stdout));
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !stderr.is_empty() {
                combined.push_str("\n[stderr]\n");
                combined.push_str(&stderr);
            }
            if out.status.success() {
                ToolResult::ok(combined)
            } else {
                ToolResult::error(format!(
                    "exit code {}: {combined}",
                    out.status.code().unwrap_or(-1)
                ))
            }
        }
        Err(e) => ToolResult::error(format!("failed to spawn: {e}")),
    }
}

// ── process_control ──────────────────────────────────────────────────────────

pub struct ProcessControlTool;

#[async_trait]
impl Tool for ProcessControlTool {
    fn name(&self) -> &str {
        "process_control"
    }
    fn description(&self) -> &str {
        "Inspect host processes. action=list returns running processes."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "action": { "type": "string", "enum": ["list"] } }
        })
    }
    async fn execute(&self, _input: Value, ctx: &ToolContext) -> ToolResult {
        #[cfg(target_os = "windows")]
        let cmd = "tasklist";
        #[cfg(not(target_os = "windows"))]
        let cmd = "ps aux | head -n 40";
        run_shell(cmd, &ctx.workspace_root).await
    }
}

// ── web_search ───────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct WebSearchTool;

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }
    fn description(&self) -> &str {
        "Search the web and return result titles, snippets and URLs."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "query": { "type": "string" } },
            "required": ["query"]
        })
    }
    async fn execute(&self, input: Value, _ctx: &ToolContext) -> ToolResult {
        let query = match str_arg(&input, "query") {
            Ok(q) => q.to_string(),
            Err(e) => return ToolResult::error(e),
        };
        let url = format!("https://duckduckgo.com/html/?q={}", urlencode(&query));
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .user_agent("HarnessLab-Desktop/1.0")
            .build()
            .unwrap_or_default();
        match client.get(&url).send().await {
            Ok(resp) => match resp.text().await {
                Ok(html) => {
                    let results = parse_ddg_results(&html);
                    if results.is_empty() {
                        ToolResult::ok("(no results)".to_string())
                    } else {
                        ToolResult::ok(results.join("\n\n"))
                    }
                }
                Err(e) => ToolResult::error(format!("read failed: {e}")),
            },
            Err(e) => ToolResult::error(format!("search request failed: {e}")),
        }
    }
}

/// Extract result title + snippet from a DuckDuckGo HTML page (best-effort).
fn parse_ddg_results(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for chunk in html.split("result__a").skip(1).take(8) {
        // The anchor text follows the next `>`.
        if let Some(gt) = chunk.find('>') {
            let after = &chunk[gt + 1..];
            if let Some(end) = after.find("</a>") {
                let title = strip_tags(&after[..end]);
                if !title.trim().is_empty() {
                    out.push(format!("• {}", title.trim()));
                }
            }
        }
    }
    out
}

// ── url_fetcher ──────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct UrlFetcherTool;

#[async_trait]
impl Tool for UrlFetcherTool {
    fn name(&self) -> &str {
        "url_fetcher"
    }
    fn description(&self) -> &str {
        "Fetch a URL and return its text content (HTML tags stripped)."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "max_chars": { "type": "integer", "description": "Truncate output (default 8000)." }
            },
            "required": ["url"]
        })
    }
    async fn execute(&self, input: Value, _ctx: &ToolContext) -> ToolResult {
        let url = match str_arg(&input, "url") {
            Ok(u) => u.to_string(),
            Err(e) => return ToolResult::error(e),
        };
        let max_chars = input
            .get("max_chars")
            .and_then(|v| v.as_u64())
            .unwrap_or(8000) as usize;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("HarnessLab-Desktop/1.0")
            .build()
            .unwrap_or_default();
        match client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status();
                match resp.text().await {
                    Ok(body) => {
                        if !status.is_success() {
                            return ToolResult::error(format!("HTTP {status}"));
                        }
                        let text = strip_tags(&body);
                        let trimmed: String = text
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ")
                            .chars()
                            .take(max_chars)
                            .collect();
                        ToolResult::ok(trimmed)
                    }
                    Err(e) => ToolResult::error(format!("read failed: {e}")),
                }
            }
            Err(e) => ToolResult::error(format!("fetch failed: {e}")),
        }
    }
}

// ── finish ───────────────────────────────────────────────────────────────────

pub struct FinishTool;

#[async_trait]
impl Tool for FinishTool {
    fn name(&self) -> &str {
        "finish"
    }
    fn description(&self) -> &str {
        "Signal that the task is complete with a final answer."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "answer": { "type": "string" } },
            "required": ["answer"]
        })
    }
    async fn execute(&self, input: Value, _ctx: &ToolContext) -> ToolResult {
        let answer = input.get("answer").and_then(|v| v.as_str()).unwrap_or("");
        ToolResult::ok(answer.to_string())
    }
}

/// Meta tool: register a declarative loop strategy into the running engine.
///
/// This is the core "self-assembly" capability of the Architect meta-agent: it
/// authors a new algorithm as data and installs it live so subsequent agents
/// (and itself) can select it immediately. The host application persists drained
/// registrations so the algorithm survives restarts.
pub struct RegisterAlgorithmTool;

#[async_trait]
impl Tool for RegisterAlgorithmTool {
    fn name(&self) -> &str {
        "register_algorithm"
    }
    fn description(&self) -> &str {
        "Author and hot-register a new declarative loop strategy into the running \
         engine. Provide a unique name, a system-prompt addendum describing the \
         reasoning pattern, and optional reflective/max_iterations flags. The new \
         strategy becomes immediately selectable as an agent's loop strategy."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Unique strategy identifier (snake_case)." },
                "description": { "type": "string", "description": "Short catalog description." },
                "addendum": { "type": "string", "description": "System-prompt guidance defining the reasoning pattern." },
                "reflective": { "type": "boolean", "description": "Run a self-reflection pass before finishing." },
                "max_iterations": { "type": "integer", "description": "Optional cap on loop iterations." }
            },
            "required": ["name", "addendum"]
        })
    }
    async fn execute(&self, input: Value, _ctx: &ToolContext) -> ToolResult {
        let name = input
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let addendum = input.get("addendum").and_then(|v| v.as_str()).unwrap_or("");
        let description = input
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let reflective = input
            .get("reflective")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let max_iterations = input
            .get("max_iterations")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);

        let spec = crate::agent::declarative::DeclarativeAlgorithm {
            kind: "loop".to_string(),
            name: name.to_string(),
            description,
            addendum: addendum.to_string(),
            reflective,
            max_iterations,
        };
        match crate::agent::declarative::register(&spec) {
            Ok(()) => ToolResult::ok(format!(
                "Registered loop strategy '{name}'. It is now selectable as an agent loop strategy."
            )),
            Err(e) => ToolResult::error(format!("registration failed: {e}")),
        }
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Strip HTML tags and collapse entities to plain text (best-effort).
fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let lower = html.to_lowercase();
    let bytes = lower.as_bytes();
    let chars: Vec<char> = html.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // Crude <script>/<style> skipping.
        if !in_script && i + 7 <= bytes.len() && (&lower[i..]).starts_with("<script") {
            in_script = true;
        }
        if in_script {
            if (&lower[i..]).starts_with("</script") || (&lower[i..]).starts_with("</style") {
                in_script = false;
            }
        }
        let c = chars[i];
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag && !in_script {
            out.push(c);
        }
        i += 1;
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// Minimal percent-encoding for query strings.
fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Platform-specific tool examples are registered via [`super::Platform`] tags.
/// The shell/process tools are cross-platform (`Any`); OS-specific helpers can
/// be added here with `fn platform(&self) -> Platform::Linux` etc.
#[allow(dead_code)]
fn _platform_marker() -> Platform {
    Platform::Any
}
