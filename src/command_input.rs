use crate::config::{ProjectConfig, WorkspaceConfig};
use crate::system_config::{EmbeddingProviderConfig, LlmProviderConfig, SystemConfig};
use inquire::{Confirm, InquireError, Select, Text};
use inquire::autocompletion::{Autocomplete, Replacement};
use inquire::CustomUserError;
use std::env;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

// ─── ANSI helpers ────────────────────────────────────────────────────────────

macro_rules! cyan   { ($s:expr) => { format!("\x1b[36m{}\x1b[0m", $s) } }
macro_rules! green  { ($s:expr) => { format!("\x1b[32m{}\x1b[0m", $s) } }
macro_rules! yellow { ($s:expr) => { format!("\x1b[33m{}\x1b[0m", $s) } }
macro_rules! bold   { ($s:expr) => { format!("\x1b[1m{}\x1b[0m",  $s) } }
macro_rules! dim    { ($s:expr) => { format!("\x1b[2m{}\x1b[0m",  $s) } }
// ─── Command list ─────────────────────────────────────────────────────────────

const COMMANDS: &[(&str, &str)] = &[
    ("/init",     "Initialize a new project"),
    ("/index",    "Index a project's codebase"),
    ("/search",   "Search the indexed codebase"),
    ("/projects", "List and manage projects"),
    ("/daemon",   "Start / stop the MCCP daemon"),
    ("/config",   "Configure providers & vector store"),
    ("/logs",     "Show and filter app logs"),
    ("/help",     "Show help and documentation"),
    ("/quit",     "Exit mccp"),
];

// ─── Autocomplete ─────────────────────────────────────────────────────────────

#[derive(Clone)]
struct CommandCompleter;

impl Autocomplete for CommandCompleter {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        if !input.starts_with('/') {
            return Ok(vec![]);
        }
        let suggestions = COMMANDS
            .iter()
            .filter(|(cmd, _)| cmd.starts_with(input))
            .map(|(cmd, desc)| format!("{:<12}  {}", cmd, desc))
            .collect();
        Ok(suggestions)
    }

    fn get_completion(
        &mut self,
        _input: &str,
        highlighted: Option<String>,
    ) -> Result<Replacement, CustomUserError> {
        Ok(match highlighted {
            Some(h) => {
                // Suggestion format: "/command    description"
                // Return only the command token so the input is clean.
                let cmd = h.split_whitespace().next().unwrap_or("").to_string();
                Replacement::Some(cmd)
            }
            None => Replacement::None,
        })
    }
}

// ─── Banner ───────────────────────────────────────────────────────────────────

fn print_banner() {
    println!();
    println!(
        "  {}  {}",
        bold!("mccp"),
        dim!("Multi-Context Code Processor")
    );
    println!(
        "  {}",
        dim!("Type / to see available commands  ·  Ctrl-C to quit")
    );
    println!();
}

fn print_workspace_hint() {
    if let Ok(ws) = WorkspaceConfig::load() {
        if let Some(proj) = ws.get_current_project() {
            println!(
                "  {} {}  {}",
                dim!("Active project:"),
                bold!(proj.name.clone()),
                dim!(format!("({})", proj.path.display()))
            );
            println!();
        } else if !ws.projects.is_empty() {
            println!(
                "  {}  {}",
                dim!("Projects:"),
                dim!(format!("{} registered — run /projects to view", ws.projects.len()))
            );
            println!();
        }
    }
}

// ─── Startup cleanup ──────────────────────────────────────────────────────────

/// Remove projects that were created by the old broken auto-init (path == "default_value").
pub fn cleanup_ghost_projects() {
    if let Ok(mut ws) = WorkspaceConfig::load() {
        let ghosts: Vec<String> = ws
            .projects
            .iter()
            .filter(|(_, p)| p.path.to_string_lossy() == "default_value")
            .map(|(name, _)| name.clone())
            .collect();
        if !ghosts.is_empty() {
            for g in &ghosts {
                ws.projects.remove(g);
            }
            if ws.default_project.as_deref().map(|d| ghosts.contains(&d.to_string())).unwrap_or(false) {
                ws.default_project = ws.projects.keys().next().cloned();
            }
            let _ = ws.save();
        }
    }
}

// ─── Entry point ─────────────────────────────────────────────────────────────

pub async fn run() -> anyhow::Result<()> {
    print_banner();
    cleanup_ghost_projects();
    print_workspace_hint();

    loop {
        let result = Text::new(&format!("{} ", cyan!("mccp")))
            .with_placeholder("type / for commands")
            .with_autocomplete(CommandCompleter)
            .prompt();

        match result {
            Ok(raw) => {
                let cmd = raw.trim().to_lowercase();
                let cmd = cmd.split_whitespace().next().unwrap_or("").to_string();
                let should_quit = dispatch(&cmd).await?;
                if should_quit {
                    break;
                }
            }
            Err(InquireError::OperationCanceled)
            | Err(InquireError::OperationInterrupted) => break,
            Err(e) => return Err(e.into()),
        }
    }

    println!("\n  {}", dim!("Goodbye! 👋"));
    Ok(())
}

async fn dispatch(cmd: &str) -> anyhow::Result<bool> {
    match cmd {
        "/init"     => { cmd_init().await?;     Ok(false) }
        "/index"    => { cmd_index().await?;    Ok(false) }
        "/search"   => { cmd_search().await?;   Ok(false) }
        "/projects" => { cmd_projects().await?; Ok(false) }
        "/daemon"   => { cmd_daemon().await?;   Ok(false) }
        "/config"   => { cmd_config().await?;  Ok(false) }
        "/logs"     => { cmd_logs().await?;    Ok(false) }
        "/help"     => { cmd_help();            Ok(false) }
        "/quit" | "/exit" | "quit" | "exit" | "q" => Ok(true),
        "" => Ok(false),
        other => {
            println!(
                "\n  {} Unknown command {}  — {}",
                yellow!("!"),
                yellow!(other),
                dim!("type / for available commands")
            );
            println!();
            Ok(false)
        }
    }
}

// ─── /init ────────────────────────────────────────────────────────────────────

async fn cmd_init() -> anyhow::Result<()> {
    println!("\n  {}", bold!(cyan!("Initialize Project")));
    println!();

    let cwd = env::current_dir()?;

    // ── path ──────────────────────────────────────────────────────────────────
    let default_path = cwd.to_string_lossy().to_string();
    let path_str = Text::new("Project path:")
        .with_default(&default_path)
        .prompt()?;
    let path = PathBuf::from(&path_str);

    // ── name ──────────────────────────────────────────────────────────────────
    let default_name = ProjectConfig::get_project_name(&path);
    let name = Text::new("Project name:")
        .with_default(&default_name)
        .prompt()?;

    // ── language ──────────────────────────────────────────────────────────────
    let detected = ProjectConfig::detect_language(&path);
    let languages = vec![
        "rust", "typescript", "javascript", "python", "go",
        "java", "cpp", "c", "csharp", "ruby", "php", "kotlin", "unknown",
    ];
    let lang_idx = languages
        .iter()
        .position(|&l| l == detected.as_str())
        .unwrap_or(0);

    let language = Select::new("Language:", languages)
        .with_starting_cursor(lang_idx)
        .with_help_message(&format!(
            "Auto-detected: {}  ·  use ↑↓ to change, Enter to confirm",
            detected
        ))
        .prompt()?;

    // ── preview + confirm ─────────────────────────────────────────────────────
    println!();
    println!("  {}  {} {}",      dim!("name:"),     bold!(name.clone()),          "");
    println!("  {}  {}",         dim!("path:"),     path.display());
    println!("  {}  {}", dim!("language:"), bold!(language));
    println!();

    let ok = Confirm::new("Initialize with these settings?")
        .with_default(true)
        .prompt()?;

    if !ok {
        println!("\n  {}", dim!("Cancelled."));
        println!();
        return Ok(());
    }

    // ── write config ──────────────────────────────────────────────────────────
    let project_config = ProjectConfig::new(name.clone(), path.clone(), language.to_string());
    let config_dir = WorkspaceConfig::get_project_config_dir(&path);
    std::fs::create_dir_all(&config_dir)?;
    project_config.save(&config_dir)?;

    let mut workspace = WorkspaceConfig::load()?;
    workspace.add_project(project_config);
    workspace.save()?;

    println!(
        "\n  {} Project {} initialized!",
        green!("✓"),
        bold!(name)
    );
    println!("  {}", dim!(format!("Config: {}", config_dir.display())));
    println!();
    Ok(())
}

// ─── /index ───────────────────────────────────────────────────────────────────

async fn cmd_index() -> anyhow::Result<()> {
    println!("\n  {}", bold!(cyan!("Index Project")));
    println!();

    let workspace = WorkspaceConfig::load()?;

    if workspace.projects.is_empty() {
        println!(
            "  {} No projects found. Run {} first.",
            yellow!("!"),
            bold!("/init")
        );
        println!();
        return Ok(());
    }

    let project_names: Vec<String> = workspace.projects.keys().cloned().collect();
    let selected = Select::new("Which project to index?", project_names).prompt()?;

    let force = Confirm::new("Force full re-index (skip change detection)?")
        .with_default(false)
        .prompt()?;

    println!();
    println!(
        "  {} Indexing {} {}",
        dim!("→"),
        bold!(selected.clone()),
        if force { dim!("(full)") } else { dim!("(incremental)") }
    );
    println!();

    let project = workspace
        .projects
        .get(&selected)
        .ok_or_else(|| anyhow::anyhow!("project '{}' not found", selected))?
        .clone();

    let sys_cfg = crate::system_config::SystemConfig::load_or_default()?;
    let indexer = match crate::indexer::Indexer::new(&sys_cfg) {
        Ok(i) => i,
        Err(e) => {
            println!(
                "  {} Cannot start indexer: {}",
                yellow!("!"),
                e
            );
            println!("  {}", dim!("Run /config to set your embedding provider (e.g. ollama)."));
            println!();
            return Ok(());
        }
    };

    let mut last_file_len = 0usize;

    let result = indexer
        .index(&project, force, |progress| {
            use crate::indexer::IndexProgress;
            match progress {
                IndexProgress::Scanning => {
                    print!("  {} Scanning files...", dim!("·"));
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                }
                IndexProgress::FilesFound { total, changed } => {
                    println!("\r  {} {} files found, {} to index", dim!("·"), total, changed);
                    if changed == 0 {
                        println!("  {} Everything up to date — no changes detected.", green!("✓"));
                    }
                }
                IndexProgress::IndexingFile { path, current, total } => {
                    // Overwrite previous line
                    let short = shorten_path(path, 50);
                    let line = format!("  {} [{}/{}] {}", dim!("→"), current, total, dim!(short));
                    let pad = " ".repeat(last_file_len.saturating_sub(line.len()));
                    print!("\r{}{}", line, pad);
                    last_file_len = line.len();
                    let _ = std::io::Write::flush(&mut std::io::stdout());
                }
                IndexProgress::FileError { path, error } => {
                    println!(
                        "\r  {} {}: {}{}",
                        yellow!("!"),
                        shorten_path(path, 40),
                        dim!(error),
                        " ".repeat(10)
                    );
                    last_file_len = 0;
                }
                IndexProgress::Done(stats) => {
                    println!("\r{}", " ".repeat(80)); // clear progress line
                    println!(
                        "  {} Indexed {} files, {} chunks in {:.1}s",
                        green!("✓"),
                        bold!(stats.files_indexed.to_string()),
                        bold!(stats.chunks_created.to_string()),
                        stats.duration_secs,
                    );
                    if stats.files_unchanged > 0 {
                        println!("  {}", dim!(format!("{} files unchanged (skipped)", stats.files_unchanged)));
                    }
                    if stats.files_removed > 0 {
                        println!("  {}", dim!(format!("{} files removed from index", stats.files_removed)));
                    }
                }
            }
        })
        .await;

    match result {
        Ok(_) => {}
        Err(e) => {
            println!("\n  {} Indexing failed: {}", yellow!("!"), e);
            if e.to_string().contains("vector store") || e.to_string().contains("Qdrant") {
                println!("  {}", dim!("Tip: start Qdrant with:  docker run -p 6333:6333 qdrant/qdrant"));
            }
        }
    }
    println!();
    Ok(())
}

// ─── /search ──────────────────────────────────────────────────────────────────

async fn cmd_search() -> anyhow::Result<()> {
    println!("\n  {}", bold!(cyan!("Search Codebase")));
    println!();

    let workspace = WorkspaceConfig::load()?;

    if workspace.projects.is_empty() {
        println!(
            "  {} No projects found. Run {} first.",
            yellow!("!"),
            bold!("/init")
        );
        println!();
        return Ok(());
    }

    let project_names: Vec<String> = workspace.projects.keys().cloned().collect();
    let project = Select::new("Project:", project_names).prompt()?;

    let query = Text::new("Query:").prompt()?;

    let limit_str = Text::new("Max results:")
        .with_default("10")
        .prompt()?;
    let limit: usize = limit_str.trim().parse().unwrap_or(10);

    println!();
    println!(
        "  {} Searching {} in {}",
        dim!("→"),
        bold!(query.clone()),
        bold!(project.clone()),
    );
    println!();

    let sys_cfg = crate::system_config::SystemConfig::load_or_default()?;
    let indexer = match crate::indexer::Indexer::new(&sys_cfg) {
        Ok(i) => i,
        Err(e) => {
            println!("  {} Cannot connect: {}", yellow!("!"), e);
            println!("  {}", dim!("Run /config to set your embedding provider."));
            println!();
            return Ok(());
        }
    };

    match indexer.search(&project, &query, limit).await {
        Ok(hits) if hits.is_empty() => {
            println!("  {} No results found.", dim!("·"));
            println!("  {}", dim!("Try /index first, or broaden your query."));
        }
        Ok(hits) => {
            for (i, hit) in hits.iter().enumerate() {
                let score_pct = (hit.score * 100.0) as u32;
                if let Some(payload) = &hit.payload {
                    let short_path = shorten_path(&payload.path, 60);
                    println!(
                        "  {}  {}  {}",
                        cyan!(format!("{:2}.", i + 1)),
                        bold!(short_path),
                        dim!(format!("lines {}-{}  score {}%", payload.start_line, payload.end_line, score_pct)),
                    );
                    // Show first 3 non-empty lines of the chunk (skip the "// path" comment)
                    let preview: Vec<&str> = payload
                        .content
                        .lines()
                        .skip(1) // skip "// path" header
                        .filter(|l| !l.trim().is_empty())
                        .take(3)
                        .collect();
                    for line in &preview {
                        println!("       {}", dim!(line));
                    }
                    println!();
                }
            }
        }
        Err(e) => {
            println!("  {} Search failed: {}", yellow!("!"), e);
            if e.to_string().contains("collection") {
                println!(
                    "  {}",
                    dim!(format!("Run /index on project '{}' first.", project))
                );
            }
        }
    }
    println!();
    Ok(())
}

/// Shorten a path for display — keep the last `max_chars` characters if needed.
fn shorten_path(path: &str, max_chars: usize) -> String {
    if path.len() <= max_chars {
        return path.to_string();
    }
    format!("…{}", &path[path.len() - max_chars..])
}


// ─── /projects ────────────────────────────────────────────────────────────────

async fn cmd_projects() -> anyhow::Result<()> {
    println!("\n  {}", bold!(cyan!("Projects")));
    println!();

    let workspace = WorkspaceConfig::load()?;

    if workspace.projects.is_empty() {
        println!("  {} No projects registered yet.", dim!("·"));
        println!("  {}", dim!("Run /init to add your first project."));
        println!();
        return Ok(());
    }

    for (name, proj) in &workspace.projects {
        let is_default = workspace.default_project.as_deref() == Some(name.as_str());
        let marker = if is_default { green!("● ") } else { dim!("○ ") };
        println!("  {}{}", marker, bold!(name.clone()));
        println!("    {}  {}", dim!("path:"),     proj.path.display());
        println!("    {}  {}", dim!("language:"), proj.language);
        if let Some(ref ts) = proj.last_indexed {
            println!("    {}  {}", dim!("indexed:"),  dim!(ts.clone()));
        }
        println!(
            "    {}  {}/{}",
            dim!("files:"), proj.indexed_files, proj.total_files
        );
        println!();
    }

    let actions = vec!["Set active project", "Remove a project", "Back"];
    let action = Select::new("Action:", actions).prompt()?;

    match action {
        "Set active project" => {
            let names: Vec<String> = workspace.projects.keys().cloned().collect();
            let pick = Select::new("Select project:", names).prompt()?;
            let mut ws = WorkspaceConfig::load()?;
            ws.set_default_project(&pick)?;
            println!(
                "\n  {} {} is now the active project.",
                green!("✓"),
                bold!(pick)
            );
        }
        "Remove a project" => {
            let names: Vec<String> = workspace.projects.keys().cloned().collect();
            let pick = Select::new("Remove which project?", names).prompt()?;
            let confirm = Confirm::new(&format!("Remove '{}'?", pick))
                .with_default(false)
                .prompt()?;
            if confirm {
                let mut ws = WorkspaceConfig::load()?;
                ws.projects.remove(&pick);
                if ws.default_project.as_deref() == Some(pick.as_str()) {
                    ws.default_project = ws.projects.keys().next().cloned();
                }
                ws.save()?;
                println!("\n  {} Project '{}' removed.", green!("✓"), pick);
            }
        }
        _ => {}
    }

    println!();
    Ok(())
}

// ─── /daemon ──────────────────────────────────────────────────────────────────

async fn cmd_daemon() -> anyhow::Result<()> {
    println!("\n  {}", bold!(cyan!("MCCP Daemon")));
    println!();

    let actions = vec!["Start daemon", "Stop daemon", "Check status"];
    let action = Select::new("Action:", actions).prompt()?;

    match action {
        "Start daemon" => {
            let host = Text::new("Bind host:")
                .with_default("127.0.0.1")
                .prompt()?;
            let port_str = Text::new("Port:")
                .with_default("7422")
                .prompt()?;
            let port: u16 = port_str.trim().parse().unwrap_or(7422);

            println!();
            println!(
                "  {} Starting daemon on {}:{}",
                dim!("→"),
                bold!(host.clone()),
                bold!(port.to_string())
            );
            println!("  {}", dim!("Daemon not yet wired — see todo/v2.md Area 7."));
        }
        "Stop daemon" => {
            println!("\n  {}", dim!("Daemon not running (or not yet wired)."));
        }
        "Check status" => {
            println!("\n  {} {}", dim!("status:"), dim!("daemon not running"));
        }
        _ => {}
    }

    println!();
    Ok(())
}

// ─── /logs ────────────────────────────────────────────────────────────────────

async fn cmd_logs() -> anyhow::Result<()> {
    println!("\n  {}", bold!(cyan!("Logs")));
    println!();

    fn expand_tilde(p: &Path) -> PathBuf {
        let s = p.to_string_lossy();
        if let Some(rest) = s.strip_prefix("~/") {
            if let Some(home) = std::env::var_os("HOME") {
                return PathBuf::from(home).join(rest);
            }
        }
        p.to_path_buf()
    }

    let default_log_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".mccp")
        .join("logs")
        .join("mccp.log");

    let path_str = Text::new("Log file path:")
        .with_default(&default_log_path.display().to_string())
        .prompt()?;
    let log_path = expand_tilde(Path::new(&path_str));

    if !log_path.exists() {
        println!(
            "  {} {}\n  {}",
            yellow!("!"),
            yellow!(format!("Log file not found: {}", log_path.display())),
            dim!("Tip: run mccp-server once to create ~/.mccp/logs/mccp.log")
        );
        println!();
        return Ok(());
    }

    let n_str = Text::new("Show last N lines:")
        .with_default("200")
        .prompt()?;
    let n: usize = n_str.trim().parse().unwrap_or(200);

    let lvl = Select::new(
        "Level filter:",
        vec!["(all)", "TRACE", "DEBUG", "INFO", "WARN", "ERROR"],
    )
    .prompt()?;
    let level = if lvl == "(all)" { None } else { Some(lvl.to_string()) };

    let contains_raw = Text::new("Contains filter (optional):")
        .with_default("")
        .prompt()?;
    let contains = if contains_raw.trim().is_empty() {
        None
    } else {
        Some(contains_raw.to_lowercase())
    };

    let follow = Confirm::new("Follow (live updates)?")
        .with_default(true)
        .prompt()?;

    let matches = |line: &str| {
        if let Some(lvl) = &level {
            if !line.contains(lvl) {
                return false;
            }
        }
        if let Some(sub) = &contains {
            if !line.to_lowercase().contains(sub) {
                return false;
            }
        }
        true
    };

    let content = std::fs::read_to_string(&log_path).unwrap_or_default();
    let all_lines: Vec<&str> = content.lines().collect();
    let start = all_lines.len().saturating_sub(n);
    for line in &all_lines[start..] {
        if matches(line) {
            println!("{}", line);
        }
    }

    if !follow {
        println!();
        return Ok(());
    }

    println!();
    println!(
        "  {} {} (Ctrl+C to stop)",
        green!("✓"),
        dim!(format!("following {}", log_path.display()))
    );

    let mut f = std::fs::File::open(&log_path)?;
    let mut offset = f.metadata()?.len();

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                let new_len = f.metadata()?.len();
                if new_len < offset {
                    offset = 0;
                }
                if new_len > offset {
                    f.seek(SeekFrom::Start(offset))?;
                    let mut buf = String::new();
                    f.read_to_string(&mut buf)?;
                    offset = new_len;
                    for line in buf.lines() {
                        if matches(line) {
                            println!("{}", line);
                        }
                    }
                }
            }
        }
    }

    println!();
    Ok(())
}

// ─── /help ────────────────────────────────────────────────────────────────────

fn cmd_help() {
    println!("\n  {}", bold!(cyan!("mccp — Multi-Context Code Processor")));
    println!();
    for (cmd, desc) in COMMANDS {
        println!("    {}  {}", cyan!(format!("{:<12}", cmd)), dim!(desc));
    }
    println!();
    println!("  {}", dim!("Tip: type / and use ↑↓ arrows to navigate, Tab to complete, Enter to run."));
    println!();
}

// ─── /config ─────────────────────────────────────────────────────────────────

/// Load (or create default) the system config from ~/.mccp/config.toml.
fn load_system_config() -> anyhow::Result<SystemConfig> {
    SystemConfig::load_or_default()
}

/// Persist the system config to ~/.mccp/config.toml.
fn save_system_config(cfg: &SystemConfig) -> anyhow::Result<()> {
    cfg.save()
}

async fn cmd_config() -> anyhow::Result<()> {
    println!("\n  {}", bold!(cyan!("Configuration")));
    println!();

    let sections = vec![
        "Vector store  (Qdrant / pgvector / Weaviate / Chroma / in-memory)",
        "Embedding provider  (Ollama · OpenAI · Azure · Cohere · HuggingFace · Custom)",
        "LLM provider  (Ollama · OpenAI · Anthropic · Azure · Groq · vLLM · Custom)",
        "Daemon settings  (port · host · log level)",
        "View current configuration",
        "Reset to defaults",
    ];

    let section = Select::new("What would you like to configure?", sections).prompt()?;

    match section {
        s if s.starts_with("Vector") => config_vector_store().await?,
        s if s.starts_with("Embedding") => config_embedding_provider().await?,
        s if s.starts_with("LLM") => config_llm_provider().await?,
        s if s.starts_with("Daemon") => config_daemon_settings().await?,
        s if s.starts_with("View") => config_show().await?,
        s if s.starts_with("Reset") => config_reset().await?,
        _ => {}
    }
    Ok(())
}

// ── Vector store ─────────────────────────────────────────────────────────────

async fn config_vector_store() -> anyhow::Result<()> {
    println!("\n  {}", bold!("Vector Store"));
    println!();

    let mut cfg = load_system_config()?;

    let drivers = vec!["qdrant", "pgvector", "weaviate", "chroma", "memory"];
    let cur_idx = drivers
        .iter()
        .position(|&d| d == cfg.vector.driver.as_str())
        .unwrap_or(0);

    let driver = Select::new("Driver:", drivers)
        .with_starting_cursor(cur_idx)
        .with_help_message("qdrant is recommended — local Docker, no external service needed")
        .prompt()?;

    cfg.vector.driver = driver.to_string();

    if driver != "memory" {
        let url = Text::new("URL:")
            .with_default(&cfg.vector.url)
            .prompt()?;
        cfg.vector.url = url;

        let api_key = Text::new("API key:")
            .with_default(if cfg.vector.api_key.is_empty() { "(none)" } else { &cfg.vector.api_key })
            .with_help_message("leave blank / '(none)' for local no-auth instances")
            .prompt()?;
        cfg.vector.api_key = if api_key == "(none)" { String::new() } else { api_key };
    }

    // HNSW tuning (advanced, optional)
    let advanced = Confirm::new("Adjust HNSW index settings? (advanced)")
        .with_default(false)
        .prompt()?;
    if advanced {
        let m_str = Text::new("HNSW m (connections per node):")
            .with_default(&cfg.vector.hnsw_m.to_string())
            .prompt()?;
        cfg.vector.hnsw_m = m_str.trim().parse().unwrap_or(cfg.vector.hnsw_m);

        let ef_str = Text::new("HNSW ef_construct (build accuracy):")
            .with_default(&cfg.vector.hnsw_ef_construct.to_string())
            .prompt()?;
        cfg.vector.hnsw_ef_construct = ef_str.trim().parse().unwrap_or(cfg.vector.hnsw_ef_construct);

        let quant = Select::new("Quantization:", vec!["scalar", "product", "none"])
            .with_starting_cursor(
                ["scalar", "product", "none"]
                    .iter()
                    .position(|&q| q == cfg.vector.quantization.as_str())
                    .unwrap_or(0),
            )
            .prompt()?;
        cfg.vector.quantization = quant.to_string();
    }

    save_system_config(&cfg)?;
    println!("\n  {} Vector store configuration saved.", green!("✓"));
    println!("  {}", dim!(format!("Driver: {}  URL: {}", cfg.vector.driver, cfg.vector.url)));
    println!();
    Ok(())
}

// ── Embedding provider ────────────────────────────────────────────────────────

async fn config_embedding_provider() -> anyhow::Result<()> {
    println!("\n  {}", bold!("Embedding Provider"));
    println!();

    let mut cfg = load_system_config()?;

    // Show existing providers
    if !cfg.embedding.providers.is_empty() {
        println!("  {} Current providers:", dim!("·"));
        for (i, p) in cfg.embedding.providers.iter().enumerate() {
            let marker = if i == 0 { green!("● (active)") } else { dim!("○") };
            println!("    {}  {}  {}", marker, bold!(p.driver.clone()), dim!(p.model.clone()));
        }
        println!();
    }

    let provider_choices = vec![
        "ollama  (local — recommended)",
        "openai",
        "azure",
        "cohere",
        "huggingface",
        "custom  (any OpenAI-compatible /v1/embeddings endpoint)",
    ];

    let choice = Select::new("Add / replace primary embedding provider:", provider_choices)
        .with_help_message("the first provider in the list is active; others are fallbacks")
        .prompt()?;

    let driver = choice.split_whitespace().next().unwrap_or("ollama");

    let prov = match driver {
        "ollama" => {
            let url = Text::new("Ollama URL:")
                .with_default("http://localhost:11434")
                .prompt()?;
            let model_choices = vec![
                "nomic-embed-text",
                "mxbai-embed-large",
                "all-minilm",
                "snowflake-arctic-embed",
                "(type custom model name)",
            ];
            let m = Select::new("Model:", model_choices).prompt()?;
            let model = if m.starts_with('(') {
                Text::new("Custom model name:").prompt()?
            } else {
                m.to_string()
            };
            EmbeddingProviderConfig { driver: "ollama".into(), model, url, api_key: String::new() }
        }
        "openai" => {
            let api_key = Text::new("OpenAI API key  (sk-…):").prompt()?;
            let model_choices = vec![
                "text-embedding-3-small",
                "text-embedding-3-large",
                "text-embedding-ada-002",
            ];
            let model = Select::new("Model:", model_choices)
                .with_starting_cursor(0)
                .prompt()?
                .to_string();
            EmbeddingProviderConfig {
                driver: "openai".into(),
                model,
                url: "https://api.openai.com/v1/embeddings".into(),
                api_key,
            }
        }
        "azure" => {
            let url = Text::new("Azure OpenAI endpoint URL:").prompt()?;
            let api_key = Text::new("Azure API key:").prompt()?;
            let model = Text::new("Deployment name / model:").with_default("text-embedding-ada-002").prompt()?;
            EmbeddingProviderConfig { driver: "azure".into(), model, url, api_key }
        }
        "cohere" => {
            let api_key = Text::new("Cohere API key:").prompt()?;
            let model = Select::new("Model:", vec!["embed-english-v3.0", "embed-multilingual-v3.0"])
                .prompt()?
                .to_string();
            EmbeddingProviderConfig {
                driver: "cohere".into(),
                model,
                url: "https://api.cohere.ai/v1/embed".into(),
                api_key,
            }
        }
        "huggingface" => {
            let url = Text::new("HuggingFace TEI endpoint URL:")
                .with_default("http://localhost:8080")
                .prompt()?;
            let model = Text::new("Model ID:")
                .with_default("BAAI/bge-large-en-v1.5")
                .prompt()?;
            EmbeddingProviderConfig { driver: "huggingface".into(), model, url, api_key: String::new() }
        }
        _ /* custom */ => {
            let url = Text::new("Endpoint URL  (/v1/embeddings compatible):").prompt()?;
            let api_key_raw = Text::new("API key  (optional, Enter to skip):").prompt()?;
            let api_key = if api_key_raw.trim().is_empty() { String::new() } else { api_key_raw };
            let model = Text::new("Model name:").prompt()?;
            EmbeddingProviderConfig { driver: "custom".into(), model, url, api_key }
        }
    };

    // Prepend as primary, keep rest as fallback
    cfg.embedding.providers.insert(0, prov.clone());
    // De-dup: remove any older provider with same driver after index 0
    cfg.embedding.providers.dedup_by(|a, b| a.driver == b.driver && { let _ = a; true });

    let dim_str = Text::new("Embedding dimensions  (0 = auto-detect on first run):")
        .with_default(&cfg.embedding.dimensions.to_string())
        .prompt()?;
    cfg.embedding.dimensions = dim_str.trim().parse().unwrap_or(cfg.embedding.dimensions);

    save_system_config(&cfg)?;
    println!(
        "\n  {} Embedding provider {} ({}) saved.",
        green!("✓"),
        bold!(prov.driver),
        dim!(prov.model)
    );
    println!();
    Ok(())
}

// ── LLM provider ─────────────────────────────────────────────────────────────

async fn config_llm_provider() -> anyhow::Result<()> {
    println!("\n  {}", bold!("LLM Provider"));
    println!();

    let mut cfg = load_system_config()?;

    if !cfg.llm.providers.is_empty() {
        println!("  {} Current providers:", dim!("·"));
        for (i, p) in cfg.llm.providers.iter().enumerate() {
            let marker = if i == 0 { green!("● (active)") } else { dim!("○") };
            println!("    {}  {}  {}", marker, bold!(p.driver.clone()), dim!(p.model.clone()));
        }
        println!();
    }

    let provider_choices = vec![
        "ollama  (local — recommended)",
        "openai",
        "anthropic",
        "azure",
        "groq",
        "vllm  (self-hosted)",
        "custom  (any OpenAI-compatible /v1/chat/completions endpoint)",
    ];

    let choice = Select::new("Add / replace primary LLM provider:", provider_choices).prompt()?;
    let driver = choice.split_whitespace().next().unwrap_or("ollama");

    let prov = match driver {
        "ollama" => {
            let url = Text::new("Ollama URL:")
                .with_default("http://localhost:11434")
                .prompt()?;
            let model_choices = vec![
                "codellama:13b",
                "codellama:7b",
                "codellama:34b",
                "llama3:8b",
                "llama3:70b",
                "deepseek-coder:6.7b",
                "mistral:7b",
                "(type custom model name)",
            ];
            let m = Select::new("Model:", model_choices).prompt()?;
            let model = if m.starts_with('(') {
                Text::new("Custom model name:").prompt()?
            } else {
                m.to_string()
            };
            LlmProviderConfig { driver: "ollama".into(), model, url, api_key: String::new() }
        }
        "openai" => {
            let api_key = Text::new("OpenAI API key  (sk-…):").prompt()?;
            let model = Select::new("Model:", vec!["gpt-4o", "gpt-4o-mini", "gpt-4-turbo", "gpt-3.5-turbo"])
                .prompt()?
                .to_string();
            LlmProviderConfig {
                driver: "openai".into(),
                model,
                url: "https://api.openai.com/v1/chat/completions".into(),
                api_key,
            }
        }
        "anthropic" => {
            let api_key = Text::new("Anthropic API key  (sk-ant-…):").prompt()?;
            let model = Select::new(
                "Model:",
                vec!["claude-3-5-sonnet-20241022", "claude-3-5-haiku-20241022", "claude-3-opus-20240229"],
            )
            .prompt()?
            .to_string();
            LlmProviderConfig {
                driver: "anthropic".into(),
                model,
                url: "https://api.anthropic.com/v1/messages".into(),
                api_key,
            }
        }
        "azure" => {
            let url = Text::new("Azure OpenAI endpoint URL:").prompt()?;
            let api_key = Text::new("Azure API key:").prompt()?;
            let model = Text::new("Deployment name:").with_default("gpt-4o").prompt()?;
            LlmProviderConfig { driver: "azure".into(), model, url, api_key }
        }
        "groq" => {
            let api_key = Text::new("Groq API key  (gsk_…):").prompt()?;
            let model = Select::new(
                "Model:",
                vec!["llama3-70b-8192", "llama3-8b-8192", "mixtral-8x7b-32768", "gemma-7b-it"],
            )
            .prompt()?
            .to_string();
            LlmProviderConfig {
                driver: "groq".into(),
                model,
                url: "https://api.groq.com/openai/v1/chat/completions".into(),
                api_key,
            }
        }
        "vllm" => {
            let url = Text::new("vLLM endpoint URL:")
                .with_default("http://localhost:8000/v1/chat/completions")
                .prompt()?;
            let model = Text::new("Model name:").with_default("mistralai/Mistral-7B-Instruct-v0.2").prompt()?;
            LlmProviderConfig { driver: "vllm".into(), model, url, api_key: String::new() }
        }
        _ /* custom */ => {
            let url = Text::new("Endpoint URL  (/v1/chat/completions compatible):").prompt()?;
            let api_key_raw = Text::new("API key  (optional, Enter to skip):").prompt()?;
            let api_key = if api_key_raw.trim().is_empty() { String::new() } else { api_key_raw };
            let model = Text::new("Model name:").prompt()?;
            LlmProviderConfig { driver: "custom".into(), model, url, api_key }
        }
    };

    cfg.llm.providers.insert(0, prov.clone());
    cfg.llm.providers.dedup_by(|a, b| a.driver == b.driver && { let _ = a; true });

    save_system_config(&cfg)?;
    println!(
        "\n  {} LLM provider {} ({}) saved.",
        green!("✓"),
        bold!(prov.driver),
        dim!(prov.model)
    );
    println!();
    Ok(())
}

// ── Daemon settings ───────────────────────────────────────────────────────────

async fn config_daemon_settings() -> anyhow::Result<()> {
    println!("\n  {}", bold!("Daemon Settings"));
    println!();

    let mut cfg = load_system_config()?;

    let port_str = Text::new("HTTP port:")
        .with_default(&cfg.daemon.http_port.to_string())
        .prompt()?;
    cfg.daemon.http_port = port_str.trim().parse().unwrap_or(cfg.daemon.http_port);

    let level = Select::new("Log level:", vec!["info", "debug", "warn", "error", "trace"])
        .with_starting_cursor(
            ["info", "debug", "warn", "error", "trace"]
                .iter()
                .position(|&l| l == cfg.daemon.log_level.as_str())
                .unwrap_or(0),
        )
        .prompt()?;
    cfg.daemon.log_level = level.to_string();

    save_system_config(&cfg)?;
    println!("\n  {} Daemon settings saved.", green!("✓"));
    println!("  {}", dim!(format!("Port: {}  Log level: {}", cfg.daemon.http_port, cfg.daemon.log_level)));
    println!();
    Ok(())
}

// ── View config ───────────────────────────────────────────────────────────────

async fn config_show() -> anyhow::Result<()> {
    let cfg = load_system_config()?;
    let path = SystemConfig::config_path()?;

    println!("\n  {} {}", bold!("Config file:"), dim!(path.display().to_string()));
    println!();
    println!("  {}  {}", dim!("daemon.port:"), bold!(cfg.daemon.http_port.to_string()));
    println!("  {}  {}", dim!("daemon.log_level:"), cfg.daemon.log_level);
    println!();
    println!("  {}  {}", dim!("vector.driver:"), bold!(cfg.vector.driver.clone()));
    println!("  {}  {}", dim!("vector.url:"), cfg.vector.url);
    if !cfg.vector.api_key.is_empty() {
        println!("  {}  {}", dim!("vector.api_key:"), dim!("set (hidden)"));
    }
    println!();
    if let Some(ep) = cfg.embedding.providers.first() {
        println!("  {}  {}", dim!("embedding.provider:"), bold!(ep.driver.clone()));
        println!("  {}  {}", dim!("embedding.model:"), ep.model.clone());
        println!("  {}  {}", dim!("embedding.dimensions:"),
            if cfg.embedding.dimensions == 0 {
                yellow!("0 (auto-detect)").to_string()
            } else {
                cfg.embedding.dimensions.to_string()
            }
        );
    }
    println!();
    if let Some(lp) = cfg.llm.providers.first() {
        println!("  {}  {}", dim!("llm.provider:"), bold!(lp.driver.clone()));
        println!("  {}  {}", dim!("llm.model:"), lp.model.clone());
    }
    println!();
    Ok(())
}

// ── Reset ─────────────────────────────────────────────────────────────────────

async fn config_reset() -> anyhow::Result<()> {
    let ok = Confirm::new("Reset ALL settings to defaults? This cannot be undone.")
        .with_default(false)
        .prompt()?;
    if ok {
        let cfg = SystemConfig::default();
        save_system_config(&cfg)?;
        println!("\n  {} Configuration reset to defaults.", green!("✓"));
    } else {
        println!("\n  {} Cancelled.", dim!("·"));
    }
    println!();
    Ok(())
}

// ─── Unit-testable pure helpers ───────────────────────────────────────────────

/// Return the command token (e.g. "/init") from a raw input string.
/// Handles trailing descriptions from the autocomplete dropdown.
pub fn parse_command(raw: &str) -> &str {
    raw.trim().split_whitespace().next().unwrap_or("")
}

/// Return true if the input string looks like it might be a /command.
pub fn is_command_input(s: &str) -> bool {
    s.trim().starts_with('/')
}


#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_command ─────────────────────────────────────────────────────────

    #[test]
    fn parse_command_strips_description() {
        // autocomplete suggestion format: "/init      Initialize a new project"
        assert_eq!(parse_command("/init      Initialize a new project"), "/init");
    }

    #[test]
    fn parse_command_plain() {
        assert_eq!(parse_command("/daemon"), "/daemon");
    }

    #[test]
    fn parse_command_trims_whitespace() {
        assert_eq!(parse_command("  /search  "), "/search");
    }

    #[test]
    fn parse_command_empty() {
        assert_eq!(parse_command(""), "");
    }

    // ── is_command_input ──────────────────────────────────────────────────────

    #[test]
    fn is_command_input_with_slash() {
        assert!(is_command_input("/init"));
        assert!(is_command_input("  /config  "));
    }

    #[test]
    fn is_command_input_without_slash() {
        assert!(!is_command_input("hello"));
        assert!(!is_command_input(""));
    }

    // ── COMMANDS completeness ─────────────────────────────────────────────────

    #[test]
    fn all_commands_start_with_slash() {
        for (cmd, _) in COMMANDS {
            assert!(cmd.starts_with('/'), "command {:?} does not start with /", cmd);
        }
    }

    #[test]
    fn commands_list_is_non_empty() {
        assert!(!COMMANDS.is_empty());
    }

    #[test]
    fn commands_include_required_entries() {
        let cmds: Vec<&str> = COMMANDS.iter().map(|(c, _)| *c).collect();
        for required in &["/init", "/index", "/search", "/projects", "/daemon", "/config", "/help", "/quit"] {
            assert!(cmds.contains(required), "missing command: {}", required);
        }
    }

    // ── CommandCompleter ──────────────────────────────────────────────────────

    #[test]
    fn completer_returns_empty_for_non_slash_input() {
        let mut c = CommandCompleter;
        let s = c.get_suggestions("hello").unwrap();
        assert!(s.is_empty());
    }

    #[test]
    fn completer_returns_all_for_slash_only() {
        let mut c = CommandCompleter;
        let s = c.get_suggestions("/").unwrap();
        assert_eq!(s.len(), COMMANDS.len());
    }

    #[test]
    fn completer_filters_correctly() {
        let mut c = CommandCompleter;
        let s = c.get_suggestions("/in").unwrap();
        // Should match /init and /index, but not /help or /quit
        assert!(s.iter().any(|x| x.starts_with("/init")));
        assert!(s.iter().any(|x| x.starts_with("/index")));
        assert!(!s.iter().any(|x| x.starts_with("/help")));
    }

    #[test]
    fn completer_get_completion_extracts_command() {
        let mut c = CommandCompleter;
        let r = c
            .get_completion("/in", Some("/init      Initialize a new project".to_string()))
            .unwrap();
        match r {
            Replacement::Some(cmd) => assert_eq!(cmd, "/init"),
            Replacement::None => panic!("expected Replacement::Some"),
        }
    }

    #[test]
    fn completer_get_completion_none_when_no_highlight() {
        let mut c = CommandCompleter;
        let r = c.get_completion("/in", None).unwrap();
        assert!(matches!(r, Replacement::None));
    }
}
