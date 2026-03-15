use std::path::Path;
use anyhow::Result;
use colored::Colorize;

// ── Skill Content ──────────────────────────────────────────────

const SKILL_SEARCH: &str = r#"---
name: lumina-search
description: Semantic code search powered by Lumina. Finds functions, patterns, and concepts across the codebase using hybrid vector + keyword search.
argument-hint: "<search query>"
allowed-tools:
  - Bash(lumina *)
  - Bash(npx lumina-search *)
  - Read
---

# Lumina Semantic Search

Search this codebase using Lumina's hybrid semantic + keyword search engine.

## Instructions

1. Run the search command:

```bash
lumina query "$ARGUMENTS" --repo .
```

If `lumina` is not in PATH, use:

```bash
npx lumina-search query "$ARGUMENTS" --repo .
```

2. Present the results clearly to the user with file paths and relevance scores.

3. If the user wants more detail on a specific result, use the Read tool to open that file at the indicated line numbers.

4. For broader searches, increase results with `-k 10`:

```bash
lumina query "$ARGUMENTS" --repo . -k 10
```
"#;

const SKILL_INDEX: &str = r#"---
name: lumina-index
description: Re-index the codebase for Lumina semantic search. Only changed files are re-processed.
allowed-tools:
  - Bash(lumina *)
  - Bash(npx lumina-search *)
---

# Lumina Index

Re-index this codebase for semantic search. Only changed files will be re-indexed (incremental).

## Instructions

Run:

```bash
lumina index --repo .
```

If `lumina` is not in PATH, use:

```bash
npx lumina-search index --repo .
```

To force a full re-index (rebuild everything from scratch):

```bash
lumina index --repo . --force
```

Report the indexing statistics to the user when complete.
"#;

const SKILL_STATUS: &str = r#"---
name: lumina-status
description: Show Lumina index status — tracked files, chunks, embedding provider, and configuration.
allowed-tools:
  - Bash(lumina *)
  - Bash(npx lumina-search *)
---

# Lumina Status

Show the current state of the Lumina search index for this project.

## Instructions

Run:

```bash
lumina status --repo .
```

If `lumina` is not in PATH, use:

```bash
npx lumina-search status --repo .
```

Present the status information clearly to the user.
"#;

const SKILL_HELP: &str = r#"---
name: lumina-help
description: Show available Lumina commands and usage guide.
---

# Lumina Help

Lumina is a semantic code search engine that gives Claude deep understanding of your codebase.

## Slash Commands

| Command | Description |
|---------|-------------|
| `/lumina-search <query>` | Search the codebase semantically. Example: `/lumina-search authentication middleware` |
| `/lumina-index` | Re-index the codebase (only changed files are processed) |
| `/lumina-status` | Show index statistics (files, chunks, provider, model) |
| `/lumina-help` | Show this help message |

## How It Works

Lumina uses **hybrid retrieval** combining:
- **Vector search** — Semantic similarity via embeddings (understands meaning)
- **Keyword search** — BM25 term matching via Tantivy (finds exact identifiers)
- **RRF fusion** — Merges and ranks results from both engines

## MCP Tools (automatic)

If the Lumina MCP server is configured (`.mcp.json`), Claude also has direct access to:
- `semantic_search` — Natural language code search
- `find_symbol` — Find functions/classes by name
- `get_file_span` — Read specific lines from a file
- `list_indexed_files` — List all indexed files

## Embedding Providers

| Provider | Cost | Quality | Dimensions |
|----------|------|---------|------------|
| **Local** (default) | Free | Good | 768 |
| **Voyage AI** | API key | Best for code | 1024 |
| **OpenAI** | API key | Good | 1536 |

## More Info

- GitHub: https://github.com/JoseZum/lumina
- npm: `npm install -g lumina-search`
"#;

// ── Public API ─────────────────────────────────────────────────

/// Install Claude Code integration (skills + MCP config).
pub fn install_claude_integration(
    repo_root: &Path,
    skip_commands: bool,
    skip_mcp: bool,
) -> Result<()> {
    if !skip_commands {
        install_skills(repo_root)?;
    }
    if !skip_mcp {
        install_mcp_config(repo_root)?;
    }
    Ok(())
}

/// Print the welcome message after init completes.
pub fn print_welcome(
    config: &crate::config::LuminaConfig,
    skip_commands: bool,
    skip_mcp: bool,
) {
    eprintln!("\n{}", "=".repeat(50).truecolor(64, 224, 208));
    eprintln!(
        "{}",
        "  Lumina is ready!".truecolor(64, 224, 208).bold()
    );
    eprintln!("{}", "=".repeat(50).truecolor(64, 224, 208));

    eprintln!("\n  {}: {} ({})",
        "Provider".cyan(),
        config.embedding_provider.display_name().white().bold(),
        config.embedding_model.yellow()
    );

    if !skip_commands {
        eprintln!("\n  {}", "Slash commands installed:".cyan());
        eprintln!("    {} - Semantic code search", "/lumina-search".white().bold());
        eprintln!("    {} - Re-index codebase", "/lumina-index".white().bold());
        eprintln!("    {} - Index statistics", "/lumina-status".white().bold());
        eprintln!("    {} - Show help", "/lumina-help".white().bold());
    }

    if !skip_mcp {
        eprintln!("\n  {} .mcp.json", "MCP server configured in".cyan());
    }

    eprintln!("\n  {}", "Next steps:".truecolor(64, 224, 208).bold());
    eprintln!("    1. Restart Claude Code (or start a new session)");
    eprintln!(
        "    2. Try: {}",
        "/lumina-search authentication middleware".white().bold()
    );
    eprintln!(
        "\n  {} Commit {} and {} to share with your team.",
        "Tip:".yellow().bold(),
        ".claude/skills/".white(),
        ".mcp.json".white()
    );
    eprintln!();
}

// ── Internal ───────────────────────────────────────────────────

fn install_skills(repo_root: &Path) -> Result<()> {
    let skills: &[(&str, &str)] = &[
        ("lumina-search", SKILL_SEARCH),
        ("lumina-index", SKILL_INDEX),
        ("lumina-status", SKILL_STATUS),
        ("lumina-help", SKILL_HELP),
    ];

    eprintln!("\n  {}", "Installing slash commands...".cyan());

    for (name, content) in skills {
        let skill_dir = repo_root.join(".claude").join("skills").join(name);
        std::fs::create_dir_all(&skill_dir)?;
        let skill_path = skill_dir.join("SKILL.md");
        std::fs::write(&skill_path, content)?;
        eprintln!(
            "    {} /{}",
            "OK".green().bold(),
            name.white()
        );
    }

    Ok(())
}

fn install_mcp_config(repo_root: &Path) -> Result<()> {
    let mcp_path = repo_root.join(".mcp.json");

    // Load existing or create fresh
    let mut config: serde_json::Value = if mcp_path.exists() {
        let content = std::fs::read_to_string(&mcp_path)?;
        serde_json::from_str(&content)
            .unwrap_or_else(|_| serde_json::json!({"mcpServers": {}}))
    } else {
        serde_json::json!({"mcpServers": {}})
    };

    // Ensure mcpServers object exists
    if !config.get("mcpServers").is_some() {
        config["mcpServers"] = serde_json::json!({});
    }

    // Add lumina server entry
    config["mcpServers"]["lumina"] = serde_json::json!({
        "command": "lumina",
        "args": ["mcp", "--repo", "."]
    });

    // Write back pretty-printed
    let formatted = serde_json::to_string_pretty(&config)?;
    std::fs::write(&mcp_path, formatted)?;

    eprintln!(
        "    {} MCP server -> {}",
        "OK".green().bold(),
        ".mcp.json".white()
    );

    Ok(())
}
