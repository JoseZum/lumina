use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Select};
use lumina::config::EmbeddingProvider;
use lumina::store::{KeywordStore, VectorStore};
use std::io::IsTerminal;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "lumina")]
#[command(about = "Semantic code search MCP server for Claude Code")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Index the current repository (or re-index changed files)
    Index {
        /// Path to the repository root (default: current directory)
        #[arg(long, default_value = ".")]
        repo: PathBuf,

        /// Force full re-index (ignore hash cache)
        #[arg(long)]
        force: bool,

        /// Embedding provider: local, voyage, openai
        #[arg(long, value_parser = parse_provider)]
        provider: Option<EmbeddingProvider>,
    },

    /// Search the index from the command line
    Query {
        /// The search query
        query: String,

        /// Number of results to return
        #[arg(short, long, default_value = "5")]
        k: usize,

        /// Path to the repository root
        #[arg(long, default_value = ".")]
        repo: PathBuf,
    },

    /// Start the MCP server (stdio transport)
    Mcp {
        /// Path to the repository root
        #[arg(long, default_value = ".")]
        repo: PathBuf,
    },

    /// Show index status and statistics
    Status {
        /// Path to the repository root
        #[arg(long, default_value = ".")]
        repo: PathBuf,
    },

    /// Initialize Lumina: index repo + install Claude Code integration
    Init {
        /// Path to the repository root (default: current directory)
        #[arg(long, default_value = ".")]
        repo: PathBuf,

        /// Embedding provider: local, voyage, openai
        #[arg(long, value_parser = parse_provider)]
        provider: Option<EmbeddingProvider>,

        /// Skip indexing (only install Claude Code integration)
        #[arg(long)]
        skip_index: bool,

        /// Skip installing Claude Code slash commands
        #[arg(long)]
        skip_commands: bool,

        /// Skip creating .mcp.json MCP server configuration
        #[arg(long)]
        skip_mcp: bool,
    },
}

fn parse_provider(s: &str) -> std::result::Result<EmbeddingProvider, String> {
    match s.to_lowercase().as_str() {
        "local" => Ok(EmbeddingProvider::Local),
        "voyage" => Ok(EmbeddingProvider::Voyage),
        "openai" => Ok(EmbeddingProvider::OpenAi),
        _ => Err(format!("Unknown provider '{}'. Use: local, voyage, openai", s)),
    }
}

fn print_banner() {
    let logo = r#"
▄▄
██                ▀▀
██ ██ ██ ███▄███▄ ██  ████▄  ▀▀█▄
██ ██ ██ ██ ██ ██ ██  ██ ██ ▄█▀██
██ ▀██▀█ ██ ██ ██ ██▄ ██ ██ ▀█▄██
"#;
    eprintln!("{}", logo.truecolor(64, 224, 208));
}

// ── Provider Selection ──

fn select_provider_interactive() -> Result<EmbeddingProvider> {
    eprintln!("\n{}\n", "Select embedding provider:".truecolor(64, 224, 208).bold());

    let items = vec![
        format!(
            "{}  -  Free, runs locally via ONNX (768 dims)\n    Model: jina-embeddings-v2-base-code",
            "Local".green().bold()
        ),
        format!(
            "{}  -  Best for code, cloud API (1024 dims)\n    Requires: VOYAGE_API_KEY",
            "Voyage AI".cyan().bold()
        ),
        format!(
            "{}  -  Cloud API, widely available (1536 dims)\n    Requires: OPENAI_API_KEY",
            "OpenAI".blue().bold()
        ),
    ];

    let selection = Select::with_theme(&ColorfulTheme::default())
        .items(&items)
        .default(0)
        .interact()
        .map_err(|e| anyhow::anyhow!("Selection failed: {}", e))?;

    let provider = match selection {
        0 => EmbeddingProvider::Local,
        1 => EmbeddingProvider::Voyage,
        2 => EmbeddingProvider::OpenAi,
        _ => unreachable!(),
    };

    Ok(provider)
}

fn ensure_provider(
    config: &mut lumina::config::LuminaConfig,
    cli_provider: Option<EmbeddingProvider>,
) -> Result<()> {
    // 1. CLI flag takes priority
    if let Some(provider) = cli_provider {
        apply_provider(config, provider);
        return Ok(());
    }

    // 2. If config.toml exists, provider was already chosen
    let config_path = config.data_dir.join("config.toml");
    if config_path.exists() {
        return Ok(());
    }

    // 3. First-time setup: interactive selection if terminal
    if std::io::stdin().is_terminal() {
        let provider = select_provider_interactive()?;

        // Warn if API key is missing for cloud providers
        match &provider {
            EmbeddingProvider::Voyage if config.voyage_api_key.is_none() => {
                eprintln!("\n{}", "Warning: VOYAGE_API_KEY is not set!".yellow().bold());
                eprintln!("Set it: {}", "export VOYAGE_API_KEY=your-key".white());
            }
            EmbeddingProvider::OpenAi if config.openai_api_key.is_none() => {
                eprintln!("\n{}", "Warning: OPENAI_API_KEY is not set!".yellow().bold());
                eprintln!("Set it: {}", "export OPENAI_API_KEY=your-key".white());
            }
            EmbeddingProvider::Local => {
                eprintln!("\n{}", "Model will be downloaded on first use (~120MB).".yellow());
            }
            _ => {}
        }

        apply_provider(config, provider);

        // Save to config.toml
        std::fs::create_dir_all(&config.data_dir)?;
        config.save().map_err(|e| anyhow::anyhow!("{}", e))?;

        eprintln!(
            "\n{} Saved to {}",
            "OK".green().bold(),
            config.data_dir.join("config.toml").display()
        );
    } else {
        // Non-interactive (MCP, pipe): default to Local
        apply_provider(config, EmbeddingProvider::Local);
    }

    Ok(())
}

fn apply_provider(config: &mut lumina::config::LuminaConfig, provider: EmbeddingProvider) {
    config.embedding_model = provider.default_model().to_string();
    config.embedding_dimensions = provider.default_dimensions();
    config.embedding_api_key = match &provider {
        EmbeddingProvider::Voyage => config.voyage_api_key.clone(),
        EmbeddingProvider::OpenAi => config.openai_api_key.clone(),
        EmbeddingProvider::Local => None,
    };
    config.embedding_provider = provider;
}

// ── Provider Lock ──

fn check_provider_lock(config: &lumina::config::LuminaConfig) -> Result<()> {
    let lock_path = config.provider_lock_path();
    if !lock_path.exists() {
        return Ok(());
    }

    let stored = std::fs::read_to_string(&lock_path)?;
    let stored = stored.trim().trim_matches('"');

    let current = config.embedding_provider.to_string();

    if stored != current {
        eprintln!(
            "{} Index was built with '{}' but config says '{}'.",
            "Error:".red().bold(),
            stored.yellow(),
            current.yellow()
        );
        eprintln!(
            "Run {} to rebuild with the new provider.",
            "lumina index --force".white().bold()
        );
        anyhow::bail!("Provider mismatch");
    }

    Ok(())
}

fn save_provider_lock(config: &lumina::config::LuminaConfig) -> Result<()> {
    let lock_path = config.provider_lock_path();
    std::fs::write(&lock_path, format!("\"{}\"", config.embedding_provider))?;
    Ok(())
}

// ── Main ──

fn main() -> Result<()> {
    // Initialize tracing to stderr (stdout reserved for MCP)
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("lumina=info".parse()?)
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Index { repo, force, provider } => {
            print_banner();
            let repo = std::fs::canonicalize(&repo)?;
            let mut config = lumina::config::LuminaConfig::load(repo)?;

            // Provider selection
            ensure_provider(&mut config, provider)?;

            // Ensure data directory exists
            std::fs::create_dir_all(&config.data_dir)?;

            if force {
                // Delete ALL index data for full re-index
                for path in [config.hashes_path(), config.provider_lock_path()] {
                    if path.exists() {
                        std::fs::remove_file(&path)?;
                    }
                }
                for path in [config.lance_path(), config.tantivy_path()] {
                    if path.exists() {
                        std::fs::remove_dir_all(&path)?;
                    }
                }
                eprintln!("{}", "Cleared all index data for full re-index".yellow());
            } else {
                // Check provider compatibility
                check_provider_lock(&config)?;
            }

            eprintln!(
                "{} using {} ({})",
                "Indexing".cyan(),
                config.embedding_provider.display_name().white().bold(),
                config.embedding_model.yellow()
            );

            let mut indexer = lumina::create_indexer(&config)?;
            let stats = indexer.index()?;

            // Save provider lock after successful index
            save_provider_lock(&config)?;

            eprintln!("\n{}", "Done!".green().bold());
            eprintln!("{}", stats);
        }

        Commands::Query { query, k, repo } => {
            print_banner();
            let repo = std::fs::canonicalize(&repo)?;
            let config = lumina::config::LuminaConfig::load(repo)?;

            if !config.data_dir.exists() {
                eprintln!("{}", "Error: No index found.".red().bold());
                eprintln!("Run {} first.", "lumina index".yellow());
                anyhow::bail!("Index not found");
            }

            eprintln!("{} {}", "Searching:".cyan(), query.white().bold());
            let engine = lumina::create_search_engine(&config)?;
            let results = engine.semantic_search(&query, k)?;
            let formatted = engine.format_results(&query, &results, config.response_token_budget);
            eprintln!("{} {} results\n", "Found".green(), results.len());
            println!("{}", formatted);
        }

        Commands::Mcp { repo } => {
            let repo = std::fs::canonicalize(&repo)?;
            let config = lumina::config::LuminaConfig::load(repo)?;

            if !config.data_dir.exists() {
                std::fs::create_dir_all(&config.data_dir)?;
            }

            lumina::mcp::run_server(config)?;
        }

        Commands::Status { repo } => {
            print_banner();
            let repo = std::fs::canonicalize(&repo)?;
            let config = lumina::config::LuminaConfig::load(repo)?;

            if !config.data_dir.exists() {
                eprintln!("{}", "No index found.".red().bold());
                eprintln!("Run {} to create one.", "lumina index".yellow());
                return Ok(());
            }

            let lance = lumina::store::lance::LanceStore::new(
                &config.lance_path(),
                config.embedding_dimensions,
            );
            let tantivy = lumina::store::tantivy_store::TantivyStore::new(&config.tantivy_path());

            let vector_count = lance.map(|s| s.count().unwrap_or(0)).unwrap_or(0);
            let keyword_count = tantivy.map(|s| s.count().unwrap_or(0)).unwrap_or(0);

            let hasher = lumina::indexer::hasher::FileHasher::new(config.hashes_path())?;

            eprintln!("{}", "Index Status".truecolor(64, 224, 208).bold());
            eprintln!("  {}: {}", "Data directory".cyan(), config.data_dir.display());
            eprintln!("  {}: {}", "Tracked files".cyan(), hasher.tracked_count());
            eprintln!("  {}: {}", "Vector chunks".cyan(), vector_count);
            eprintln!("  {}: {}", "Keyword chunks".cyan(), keyword_count);
            eprintln!("  {}: {}", "Provider".cyan(), config.embedding_provider.display_name());
            eprintln!("  {}: {}", "Model".cyan(), config.embedding_model);
            eprintln!("  {}: {}", "Dimensions".cyan(), config.embedding_dimensions);
            eprintln!(
                "  {}: {}",
                "API key".cyan(),
                match config.embedding_provider {
                    EmbeddingProvider::Local => "not needed".green().to_string(),
                    _ => {
                        if config.embedding_api_key.is_some() {
                            "configured".green().to_string()
                        } else {
                            format!(
                                "NOT SET (set {})",
                                config.embedding_provider.env_var().unwrap_or("API_KEY")
                            ).red().to_string()
                        }
                    }
                }
            );
        }

        Commands::Init { repo, provider, skip_index, skip_commands, skip_mcp } => {
            print_banner();
            let repo = std::fs::canonicalize(&repo)?;
            let mut config = lumina::config::LuminaConfig::load(repo.clone())?;

            // Provider selection
            ensure_provider(&mut config, provider)?;
            std::fs::create_dir_all(&config.data_dir)?;
            config.save().map_err(|e| anyhow::anyhow!("{}", e))?;

            // Index (unless skipped)
            if !skip_index {
                eprintln!(
                    "\n  {} using {} ({})",
                    "Indexing".cyan(),
                    config.embedding_provider.display_name().white().bold(),
                    config.embedding_model.yellow()
                );
                let mut indexer = lumina::create_indexer(&config)?;
                let stats = indexer.index()?;
                save_provider_lock(&config)?;
                eprintln!("{}", stats);
            }

            // Install Claude Code integration
            lumina::init::install_claude_integration(
                &repo,
                skip_commands,
                skip_mcp,
            )?;

            // Welcome message
            lumina::init::print_welcome(&config, skip_commands, skip_mcp);
        }
    }

    Ok(())
}
