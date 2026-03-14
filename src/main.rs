use anyhow::Result;
use clap::{Parser, Subcommand};
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
}

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
        Commands::Index { repo, force } => {
            let repo = std::fs::canonicalize(&repo)?;
            let mut config = lumina::config::LuminaConfig::load(repo)?;

            if force {
                // Delete hash cache to force full re-index
                let hashes_path = config.hashes_path();
                if hashes_path.exists() {
                    std::fs::remove_file(&hashes_path)?;
                    eprintln!("Cleared hash cache for full re-index.");
                }
            }

            // Ensure data directory exists
            std::fs::create_dir_all(&config.data_dir)?;

            let mut indexer = lumina::create_indexer(&config)?;
            let stats = indexer.index()?;
            eprintln!("{}", stats);
        }

        Commands::Query { query, k, repo } => {
            let repo = std::fs::canonicalize(&repo)?;
            let config = lumina::config::LuminaConfig::load(repo)?;

            if !config.data_dir.exists() {
                anyhow::bail!(
                    "No index found at {}. Run `lumina index` first.",
                    config.data_dir.display()
                );
            }

            let engine = lumina::create_search_engine(&config)?;
            let results = engine.semantic_search(&query, k)?;
            let formatted = engine.format_results(&query, &results, config.response_token_budget);
            println!("{}", formatted);
        }

        Commands::Mcp { repo } => {
            let repo = std::fs::canonicalize(&repo)?;
            let config = lumina::config::LuminaConfig::load(repo)?;

            if !config.data_dir.exists() {
                std::fs::create_dir_all(&config.data_dir)?;
            }

            let engine = lumina::create_search_engine(&config)?;
            lumina::mcp::run_server(engine, config.response_token_budget)?;
        }

        Commands::Status { repo } => {
            let repo = std::fs::canonicalize(&repo)?;
            let config = lumina::config::LuminaConfig::load(repo)?;

            if !config.data_dir.exists() {
                eprintln!("No index found at {}.", config.data_dir.display());
                eprintln!("Run `lumina index` to create one.");
                return Ok(());
            }

            let lance = lumina::store::lance::LanceStore::new(&config.lance_path());
            let tantivy = lumina::store::tantivy_store::TantivyStore::new(&config.tantivy_path());

            let vector_count = lance.map(|s| s.count().unwrap_or(0)).unwrap_or(0);
            let keyword_count = tantivy.map(|s| s.count().unwrap_or(0)).unwrap_or(0);

            let hasher = lumina::indexer::hasher::FileHasher::new(config.hashes_path())?;

            eprintln!("Lumina Index Status");
            eprintln!("  Data directory: {}", config.data_dir.display());
            eprintln!("  Tracked files: {}", hasher.tracked_count());
            eprintln!("  Vector store chunks: {}", vector_count);
            eprintln!("  Keyword store chunks: {}", keyword_count);
            eprintln!("  Embedding model: {}", config.voyage_model);
            eprintln!(
                "  API key: {}",
                if config.voyage_api_key.is_some() {
                    "configured"
                } else {
                    "NOT SET (set VOYAGE_API_KEY)"
                }
            );
        }
    }

    Ok(())
}
