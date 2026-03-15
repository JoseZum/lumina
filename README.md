# Lumina

**Semantic code search for Claude Code**

Give Claude deep understanding of your codebase. Instead of reading files one-by-one, Claude searches your code semantically — like having a senior developer who's read every line.

```
You:    "Find where authentication is handled"
Claude: Found 5 results:

  src/auth/middleware.rs:23     verify_jwt_token()        0.94
  src/auth/provider.rs:89      authenticate_user()        0.91
  src/routes/login.rs:12       handle_login()             0.87
```

---

## Quick Start

### 1. Install

```bash
npm install -g lumina-search
```

> **Requirements:** Node.js >= 18. Windows users need [WSL](https://learn.microsoft.com/en-us/windows/wsl/install).

### 2. Initialize

```bash
cd your-project
lumina init
```

This will:
- Ask you to pick an embedding provider (Local is free, no API key needed)
- Index your codebase
- Install Claude Code slash commands
- Configure the MCP server

### 3. Use in Claude Code

Restart Claude Code, then use the slash commands:

```
/lumina-search authentication middleware
/lumina-search database connection pooling
/lumina-search how are errors handled
```

That's it! Lumina is ready.

---

## Slash Commands

After running `lumina init`, these commands are available inside Claude Code:

| Command | What it does |
|---------|-------------|
| `/lumina-search <query>` | Search your code semantically |
| `/lumina-index` | Re-index (only changed files) |
| `/lumina-status` | Show index stats |
| `/lumina-help` | Show help |

### Examples

```
/lumina-search JWT token validation
/lumina-search how does the payment flow work
/lumina-search WebSocket connection handling
/lumina-search where are environment variables loaded
```

Lumina understands **meaning**, not just keywords. Searching for "authentication" finds `verify_token()`, `login_handler()`, and `JwtMiddleware` — even if the word "authentication" never appears in the code.

---

## MCP Server (Claude Desktop)

Lumina also works as an MCP server, giving Claude direct access to search tools.

### Automatic setup (via `lumina init`)

Running `lumina init` creates a `.mcp.json` file in your project. Claude Code picks this up automatically.

### Manual setup (Claude Desktop)

Add to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "lumina": {
      "command": "lumina",
      "args": ["mcp", "--repo", "/path/to/your/project"]
    }
  }
}
```

**Config file locations:**
- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows: `%APPDATA%\Claude\claude_desktop_config.json`
- Linux: `~/.config/Claude/claude_desktop_config.json`

### MCP Tools

When connected, Claude gets these tools automatically:

| Tool | Description |
|------|-------------|
| `semantic_search` | Natural language code search |
| `find_symbol` | Find functions/classes by name |
| `get_file_span` | Read specific lines from a file |
| `list_indexed_files` | List all indexed files |

---

## CLI Reference

### `lumina init`

Set up everything in one command.

```bash
lumina init                          # Interactive setup
lumina init --provider local         # Skip provider selection
lumina init --skip-index             # Only install Claude integration
lumina init --skip-commands          # Don't install slash commands
lumina init --skip-mcp               # Don't create .mcp.json
```

### `lumina index`

Index or re-index your codebase.

```bash
lumina index --repo .                # Index current directory
lumina index --repo . --force        # Full re-index (delete cache)
lumina index --repo . --provider openai  # Use OpenAI embeddings
```

### `lumina query`

Search from the command line.

```bash
lumina query "authentication middleware" --repo .
lumina query "database connection" --repo . -k 10
```

### `lumina status`

Check index health.

```bash
lumina status --repo .
```

### `lumina mcp`

Start the MCP server (used by Claude, not called directly).

```bash
lumina mcp --repo .
```

---

## Embedding Providers

Lumina supports 3 embedding providers. Choose during `lumina init`:

| Provider | Cost | Quality | Setup |
|----------|------|---------|-------|
| **Local** (default) | Free | Good | Nothing needed |
| **Voyage AI** | API key | Best for code | `export VOYAGE_API_KEY=your-key` |
| **OpenAI** | API key | Good | `export OPENAI_API_KEY=your-key` |

### Local (recommended for getting started)

Uses [fastembed](https://github.com/Anush008/fastembed-rs) with the `jina-embeddings-v2-base-code` model. Runs entirely on your machine. Downloads the model (~120MB) on first use.

### Voyage AI (recommended for production)

Best quality for code search. Get an API key at [voyageai.com](https://www.voyageai.com/).

```bash
export VOYAGE_API_KEY="pa-your-key"
lumina init --provider voyage
```

### OpenAI

Uses `text-embedding-3-small`. Get an API key at [platform.openai.com](https://platform.openai.com/).

```bash
export OPENAI_API_KEY="sk-your-key"
lumina init --provider openai
```

---

## Supported Languages

| Language | Extensions |
|----------|-----------|
| Python | `.py` |
| Rust | `.rs` |
| TypeScript | `.ts`, `.tsx` |
| JavaScript | `.js`, `.jsx` |
| Go | `.go` |
| Java | `.java` |

Lumina uses [tree-sitter](https://tree-sitter.github.io/) for AST-aware code parsing. It understands functions, classes, methods, and structs — not just lines of text.

---

## How It Works

```
                    lumina init
                        |
        +---------------+---------------+
        |                               |
   Index codebase              Install integration
        |                        |            |
   +---------+            Slash commands  .mcp.json
   |         |            (.claude/skills/)
Parse code  Embed
(tree-sitter) (vectors)
   |         |
   +----+----+
        |
  Store in dual index
  |              |
LanceDB       Tantivy
(vectors)     (keywords)

                lumina query "auth"
                        |
        +---------------+---------------+
        |                               |
  Vector search (LanceDB)    Keyword search (Tantivy)
  "semantic similarity"       "exact term matching"
        |                               |
        +---------------+---------------+
                        |
                  RRF Fusion
              (merge & rank results)
                        |
                  Top-k results
```

**Hybrid retrieval** combines the best of both worlds:
- Vector search finds `verify_token()` when you search "authentication"
- Keyword search finds `UserService` when you search "userservice"
- RRF fusion merges both result sets into one ranked list

---

## Configuration

Lumina stores its config in `.lumina/config.toml`:

```toml
embedding_provider = "local"
embedding_model = "jinaai/jina-embeddings-v2-base-code"
embedding_dimensions = 768
embedding_batch_size = 128

max_chunk_tokens = 500
min_chunk_tokens = 50
max_file_size = 1048576

search_k_vector = 30
search_k_keyword = 30
rrf_k = 60
response_token_budget = 2000
```

Most defaults work well. The main thing you might change is `response_token_budget` (how much code Claude gets per search).

---

## Troubleshooting

### "No index found"

Run `lumina init` or `lumina index --repo .` first.

### "Provider mismatch"

You changed your embedding provider after indexing. Run:

```bash
lumina index --repo . --force
```

This rebuilds the entire index with the new provider.

### Slash commands not showing up

1. Make sure `.claude/skills/` directory exists in your project
2. Restart Claude Code
3. Type `/lumina` and check if commands appear in autocomplete

### MCP server not connecting

1. Check `.mcp.json` exists in your project root
2. Make sure `lumina` is in your PATH: `which lumina` or `where lumina`
3. Restart Claude Code

### Windows: "lumina not found"

Lumina on Windows runs through WSL. Make sure:

1. WSL is installed: `wsl --version`
2. Ubuntu is installed: `wsl --list`

### Build errors during install

If the pre-built binary isn't available for your platform:

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Rebuild
npm rebuild lumina-search
```

---

## FAQ

**Q: Does Lumina send my code to the cloud?**
A: Only if you choose Voyage AI or OpenAI as your embedding provider. With **Local** mode, everything stays on your machine.

**Q: How big is the index?**
A: Roughly 1-5 MB per 1000 files, depending on code density.

**Q: Can I use Lumina with Claude Desktop (not just Claude Code)?**
A: Yes! Set up the MCP server in `claude_desktop_config.json`. See the [MCP Server](#mcp-server-claude-desktop) section.

**Q: What happens when I change files?**
A: Run `/lumina-index` or `lumina index --repo .`. Only changed files are re-processed (SHA-256 caching).

**Q: Can I use it with other AI tools?**
A: Lumina implements the standard [MCP protocol](https://modelcontextprotocol.io/), so any MCP-compatible tool can connect to it.

---

## License

Apache 2.0

---

## Credits

Built with [tree-sitter](https://tree-sitter.github.io/), [LanceDB](https://lancedb.com/), [Tantivy](https://github.com/quickwit-oss/tantivy), [fastembed](https://github.com/Anush008/fastembed-rs), [Voyage AI](https://www.voyageai.com/), and the [Model Context Protocol](https://modelcontextprotocol.io/).
