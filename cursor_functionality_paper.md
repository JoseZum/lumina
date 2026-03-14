# Inside semantic code search: how Cursor, Cody, and Copilot actually work

**Cursor, Sourcegraph Cody, and GitHub Copilot each employ fundamentally different architectures for semantic code search, but they converge on a shared paradigm: two-stage retrieve-then-rerank pipelines that combine vector embeddings, keyword search, and AST-aware code graphs.** Cursor has built the most vertically integrated system — a custom embedding model trained on agent session traces, Turbopuffer as its vector database at 100B+ vectors, and Priompt for JSX-based priority-driven context packing. Sourcegraph Cody leans on its decade-old code search infrastructure (Zoekt trigram engine + SCIP symbol graphs) rather than embeddings alone, having largely moved away from vector search at enterprise scale. Copilot uses a lightweight Jaccard-similarity sliding window over open tabs, emphasizing speed over deep retrieval. Understanding these architectural differences is critical for anyone building code-aware AI tooling — the gap between naive RAG and production code search is enormous.

---

## Cursor's pipeline: custom embeddings, Turbopuffer, and Merkle tree sync

Cursor runs a full **client-server RAG pipeline** where the VS Code fork handles file scanning and chunking locally, while embedding generation and vector storage happen server-side on AWS. The architecture breaks into several precise stages.

**Chunking uses AST-aware "syntactic chunks."** Cursor splits code at semantic boundaries — functions, classes, and logical blocks — rather than fixed token windows. Multiple technical analyses describe this as tree-sitter-based parsing, where raw code is converted to an Abstract Syntax Tree, then sibling AST nodes are merged into larger chunks up to a token limit. Cursor's official blog describes these as "syntactic chunks," and while the company hasn't explicitly named tree-sitter, the approach is functionally identical. The key detail: sibling node merging prevents creating too many tiny chunks while respecting semantic boundaries. Exact chunk size thresholds are undisclosed, but they must stay within the embedding model's input limit.

**The embedding model is entirely custom, trained on agent behavior.** Cursor's November 2025 blog post confirmed they trained their own embedding model using a novel data source: agent session traces. When a Cursor agent works through a coding task, it performs multiple searches and file accesses. An LLM ranks what content would have been most helpful at each step, and the embedding model is trained to align similarity scores with these LLM-generated rankings. This creates a feedback loop where the model learns from how agents actually navigate codebases, not from generic code similarity benchmarks. Earlier versions used OpenAI's embedding API (confirmed by co-founder Michael Truell in 2023), but the custom model now runs on **Fireworks** and **Baseten** infrastructure. The results: **12.5% higher accuracy** on average, with code retention increasing by **2.6% on large codebases** (1,000+ files).

**Turbopuffer stores 100B+ vectors across 10M+ namespaces.** Cursor migrated to Turbopuffer in November 2023, achieving a **20× cost reduction** (95% savings). Each `(user_id, codebase)` pair maps to a separate Turbopuffer namespace. Active namespaces live in memory/NVMe cache; inactive ones fade to object storage. Critically, **raw source code is never stored remotely** — only embedding vectors, obfuscated file paths (each path segment encrypted with a client-side key), and line ranges. At query time, Turbopuffer returns ranked results with obfuscated paths and line ranges; the client decrypts paths and reads actual code from the local filesystem.

**Merkle tree synchronization enables incremental updates.** Every 3–5 minutes, Cursor computes a SHA-256 Merkle tree of file hashes and syncs it to the server. Only branches where hashes differ trigger re-chunking and re-embedding. For a 50K-file workspace, a naive approach would transfer ~3.2 MB of filenames and hashes; the Merkle tree approach transfers far less. Embedding results are cached in AWS keyed by chunk content hash, so identical chunks across users hit the cache. A particularly clever optimization: **cross-user index reuse**. Clones of the same codebase average 92% similarity, so new users compute a similarity hash (simhash) from their Merkle tree, and the server searches existing indexes to copy as a starting point. This drops median time-to-first-query from **7.87 seconds to 525 milliseconds**.

---

## Sourcegraph Cody: Zoekt, SCIP, and why they moved away from embeddings

Cody's architecture diverges sharply from Cursor's embedding-first approach. Sourcegraph discovered that **embeddings-only retrieval doesn't scale to 100,000+ repositories** — the privacy implications of sending code to OpenAI for embedding, the admin complexity of maintaining vector indexes, and the scaling limitations of vector search all drove them toward their existing code search infrastructure.

**The retrieval pipeline runs four complementary retrievers in parallel.** Cody's context engine uses a two-stage architecture inspired by recommendation systems (Spotify, YouTube):

- **Zoekt keyword retriever**: Sourcegraph's trigram-based search engine (originally from Google) indexes code by extracting 3-character sequences and storing byte offsets. It delivers sub-50ms search on multi-gigabyte codebases with BM25 scoring. A "query understanding" step uses a lightweight LLM to rewrite natural-language queries into search terms, extract entities (file paths, symbols, key terms), and handle foreign-language queries.
- **Embedding-based retriever**: Uses OpenAI text-embedding-ada-002 (1536 dimensions) for enterprise or Sourcegraph's own st-multi-qa-mpnet-base-dot-v1 for local use. Retained as an option but no longer the primary enterprise path.
- **Graph-based retriever**: Uses SCIP (Sourcegraph Code Intelligence Protocol) to build and traverse dependency graphs. Employs a "Repo-level Semantic Graph" with expand-and-refine traversal and link prediction to find all call sites, class implementations, and dependency chains.
- **Local context retriever**: Examines open files, cursor position, recent git history, open tabs. For autocomplete specifically, uses **Jaccard similarity with a sliding window** — no embeddings needed, much faster.

The key design principle: **retrievers must be complementary**, each surfacing distinct types of relevant code. Keyword search finds exact references; semantic search finds conceptually related code; graph retrieval identifies structural dependencies.

**SCIP powers precise cross-repository code navigation.** SCIP is a Protobuf-based protocol using human-readable string symbol IDs (replacing the earlier LSIF's opaque numeric IDs). Symbol format: `<scheme> <manager> <package-name> <version> <descriptor>+`. Language-specific indexers exist for TypeScript, Java/Scala/Kotlin, Python, C/C++, Go, and more. Over **45,000 repositories** on sourcegraph.com have precise code navigation, processing 4,000+ SCIP uploads per day. SCIP indexes are **4× smaller** than equivalent LSIF when gzip-compressed, with **10× CI speedup** when replacing lsif-node with scip-typescript. Meta's integration found SCIP "8× smaller, processed 3× faster" than LSIF.

**Ranking uses a transformer encoder model with pointwise scoring.** After retrieval, a transformer model predicts per-item relevance scores — simple pointwise ranking rather than pairwise or listwise. An adapted BM25 function provides additional signals. The ranking layer acts as both a merge step for items from different retrievers and the final filter. Sourcegraph frames context selection as a **knapsack problem**: given a token budget, select the combination of items maximizing total relevance. Items below a threshold are excluded entirely — irrelevant context wastes tokens and actively confuses the LLM.

**Context windows are allocated as fixed budgets.** Cody allocates **30,000 tokens** for user-defined context and **15,000 tokens** for continuous conversation context, with support for up to 1M tokens via Claude Sonnet 4 or Gemini Flash. The autocomplete pipeline is architecturally separate: it uses tree-sitter for intent detection (single-line vs multi-line completion), Jaccard similarity for local context gathering, and reciprocal rank fusion for combining snippet lists — all running against smaller, faster models (DeepSeek v2 replaced StarCoder, cutting P75 latency by 350ms).

---

## Symbol graphs turn code structure into a queryable knowledge graph

Vector search finds semantically similar text chunks but fundamentally cannot answer structural questions: "What functions call `UserService.create_user`?", "Show all classes implementing the Repository interface," or "What would break if I change this function signature?" Symbol graphs fill this gap by turning code into a traversable graph.

**AST-based symbol graphs work by augmenting parse trees with semantic edges.** The construction pipeline: (1) Parse source files using tree-sitter into ASTs, (2) Extract semantic entities — functions, classes, variables, imports, (3) Build cross-file edges — call relationships, inheritance chains, import dependencies, data flows. The result is stored in a graph database (Neo4j, KuzuDB, or in-memory structures like rustworkx) and queried with Cypher or custom traversal algorithms.

Several production and open-source implementations demonstrate this approach. **Codegen** (codegen.com) uses tree-sitter + rustworkx for a two-stage graph: AST parsing followed by custom logic building cross-file symbol relationships, enabling constant-time lookups for dependencies and usages. **CodePrism** creates a language-agnostic "Universal AST" with nodes for Module, Class, Function, Import, Call, Reference, Assignment, and DataFlow, claiming 1,000+ files/sec indexing. **Joern's Code Property Graph** merges AST, control flow graph, and program dependence graph into a unified representation used primarily for security analysis. The **tree-sitter-graph** library provides an official DSL for constructing arbitrary graph structures from tree-sitter parse trees.

**Aider's repo map is a practical example of graph-ranked context selection.** Aider uses tree-sitter to extract symbol definitions, builds a dependency graph where files are nodes and edges connect files with dependencies, then applies a **graph ranking algorithm** to select the most important identifiers that fit within a token budget (default 1,024 tokens). This produces a map showing function signatures and key identifiers — full bodies are only included for files actively being edited. The ranking ensures that heavily-referenced symbols appear first, maximizing information density per token.

**GitHub's Stack Graphs** attempted per-file incremental analysis without build tools, using tree-sitter for parsing and a declarative DSL for name-binding rules. Path-finding on the graph resolves definition-reference relationships. However, reports suggest the project has seen reduced investment, with GitHub's Precise Code Navigation partially unshipped.

---

## Rerankers: the critical second stage that cross-encoders make possible

The distinction between bi-encoders and cross-encoders is the single most important architectural decision in code search quality. **Bi-encoders** (embedding models) encode query and documents separately into fixed-dimensional vectors, enabling fast similarity computation via cosine distance — but they compress information losfully. **Cross-encoders** encode query-document pairs together through a full transformer pass, capturing fine-grained token-level interactions that bi-encoders miss. Cross-encoders are **3–10× more accurate** for relevance scoring but computationally infeasible for full corpus search. On a V100 GPU, BERT-based cross-encoding of 40M records would take 50+ hours per query.

This motivates the universal two-stage pipeline: fast bi-encoder or BM25 retrieves top-K candidates (typically 50–200) from millions of documents in under 100ms, then a cross-encoder rescores just these K candidates. Databricks reports reranking improves retrieval quality by up to **48%**. The optimal candidate set is around 50 documents for latency-sensitive applications, 100–200 for comprehensive search.

**Cursor uses reranking as an explicit step in its @Codebase pipeline.** After initial retrieval gathers candidate files and chunks, a reranking step reorders them by relevance to the query before context assembly. Users can toggle between "reranking" and "embeddings" search behavior, with reranking recommended for full-codebase searches. The specific reranker model is undisclosed but described as a post-processing filter.

**Sourcegraph Cody uses a transformer encoder model trained for pointwise relevance prediction.** Each candidate receives an independent relevance score (0–1), and items are ranked by score. This is the simplest ranking approach but sufficient when combined with the diverse retriever set.

For those building custom systems, the relevant model landscape includes **CodeBERT** (Microsoft's bimodal pre-trained model for NL + PL), **GraphCodeBERT** (extends CodeBERT with data flow graphs), **UniXcoder** (unified cross-modal model achieving 72.0 MRR on CodeSearchNet Python), **Cohere Rerank** (commercial cross-encoder supporting 100+ languages), **Jina rerankers** (optimized for long sequences with ALiBi attention), and **mixedbread mxbai-rerank** (open-source, Apache 2.0, based on Qwen-2.5, handling code/JSON/text up to 8K tokens). The `rerankers` Python library from Answer.AI provides a unified interface across all these models.

---

## Context selection: the art of maximizing relevance per token

How these systems decide what to send to the LLM may matter more than the retrieval itself. Sending too much irrelevant context wastes tokens and actively degrades output quality — Sourcegraph explicitly notes that irrelevant context performs worse than no context at all.

**GitHub Copilot uses a priority queue with Jaccard similarity scoring.** The pipeline: (1) scan open editor tabs, recently edited files, same-directory files, and import graph; (2) break files into **60-line sliding windows**; (3) score each window against cursor-surrounding code using Jaccard similarity (fast token-overlap, no embeddings); (4) keep only the highest-scoring window per file; (5) assemble the prompt with strict priority ordering — prefix code above cursor (highest), suffix below cursor, ranked neighboring file snippets, imports, constants. A token estimator continuously monitors the **~8K–16K budget**, trimming lowest-priority items. Acceptance rate improved from ~20% to **35%** largely through prompt assembly optimizations.

**Cursor's Priompt library implements priority-based prompt tree pruning.** Priompt (Priority + Prompt) is an open-source JSX-based system where each component has a numeric priority. When the assembled prompt exceeds the context window, lowest-priority items are pruned automatically. The prompt is **rebuilt from scratch on every message** (stateless LLM APIs demand this), including system instructions, model-specific directives, user message, selected chat history, MCP tool metadata (up to 40 tools), and code/file context. Heavy prompt caching is applied for unchanged sections. This design is inspired by responsive web design — content adapts to different "screen sizes" (context windows).

**Emerging research on code-specific compression is promising.** LongCodeZip (2025) introduces the first framework specifically for long-context code compression: a dual-stage approach using (1) function-level ranking via conditional perplexity to retain only the most relevant functions, then (2) block-level selection within retained functions using perplexity-based segmentation. A 0.5B parameter model handles compression without sacrificing quality. The "lost-in-the-middle" effect — where LLMs underweight content in the middle of prompts — means placement matters: critical context should appear first and last.

**Function signatures vs full bodies is a spectrum, not a binary.** Aider sends function signatures and key identifiers in its repo map (default 1K tokens), reserving full bodies only for actively edited files. Sourcegraph's graph retriever can return call chains as signatures without implementation details. The RACG survey identifies three retrieval strategies: Header2Code (signatures as queries), NL2Code (comments as queries), and NL2NL (retrieving similar comments then associated code) — with NL2NL performing best.

---

## Open source implementations worth studying

Several repositories replicate or extend Cursor's architecture with varying fidelity:

- **srag** (github.com/wrxck/srag) — Rust-based semantic code search with tree-sitter chunking, SQLite + HNSW vector index, an MCP server, and hybrid search using reciprocal rank fusion. Closest to a production-quality Cursor alternative.
- **claude-context** (github.com/zilliztech/claude-context, 5.3K stars) — The most popular MCP-native code search tool. Uses Milvus/Zilliz for hybrid BM25 + dense vector search, AST-based splitting with automatic fallback, incremental indexing via Merkle trees, and claims ~40% token reduction.
- **code-splitter** (github.com/wangxj03/code-splitter) — Rust crate implementing tree-sitter AST-based code chunking, paired with OpenAI text-embedding-3-small and Qdrant.
- **code-sage** (github.com/faxioman/code-sage) — Rust MCP server with tree-sitter chunking, hybrid BM25 + vector search, supporting 60+ file types.
- **Priompt** (github.com/anysphere/priompt) — Cursor's own open-source prompt prioritization library, directly usable for context window management.
- **code-graph-rag** (github.com/vitali87/code-graph-rag) — Parses multi-language repos into knowledge graphs with Cypher query support.
- **coa-codesearch-mcp** (github.com/anortham/coa-codesearch-mcp) — .NET + Lucene-powered code search with tree-sitter type extraction for 25 languages, providing `goto_definition` and `find_references` tools.

---

## Building an MCP server for code search

The Model Context Protocol provides the standardized interface for exposing code search tools to Claude Code, Cursor, and other MCP-aware clients. The protocol runs on JSON-RPC 2.0 with three core primitives: **Tools** (executable functions), **Resources** (read-only data), and **Prompts** (reusable templates). Transport options are stdio (local, single-client, recommended for development) and Streamable HTTP (remote, multi-client, supports OAuth 2.1).

**The optimal tool surface for a code search MCP server includes 5–7 tools**: `index_codebase`, `search_code` (semantic + keyword), `search_symbols` (definition lookup), `get_file` (with line range support), `find_references`, `list_files`, and `get_index_status`. Tool annotations should set `readOnlyHint: true` for all search operations. A critical performance constraint: tool definitions consume tokens from the LLM's context window. A server with 20 verbose tool definitions can consume **14,000+ tokens** before any actual content. Keep descriptions concise and limit total tool count.

The Sourcegraph MCP server (sourcegraph.com/mcp) represents the most complete production implementation, exposing exact keyword search, semantic search, `goto_definition`, file retrieval with line ranges, diff search, and repository search — all backed by enterprise-scale Zoekt + SCIP infrastructure. For self-hosted alternatives, claude-context (Zilliz) and srag offer the strongest starting points for building hybrid vector + keyword search with tree-sitter-aware chunking. The official TypeScript SDK (`@modelcontextprotocol/sdk`) with Zod schema validation is the recommended foundation, with v1.x stable for production use.

---

## Conclusion

The architecture of production code search systems reveals a clear hierarchy of techniques. **Pure vector search is necessary but insufficient** — every production system augments it with keyword search (Zoekt trigrams, BM25), structural analysis (SCIP symbol graphs, tree-sitter AST parsing), and aggressive reranking. The most impactful innovations are not in embedding models but in context engineering: Cursor's Priompt priority system, Sourcegraph's knapsack-style token budgeting, and Copilot's Jaccard-similarity sliding windows all prioritize information density over completeness. 

Three insights stand out from this analysis. First, **Cursor's decision to train embeddings on agent session traces** rather than generic code similarity is architecturally novel — it optimizes for how AI agents actually navigate codebases. Second, **Sourcegraph's retreat from embeddings at enterprise scale** signals that vector search alone cannot handle 100K+ repository environments; hybrid retrieval with existing search infrastructure outperforms. Third, the emergence of **MCP as the standard interface** for code search tools means the retrieval backend is increasingly decoupled from the AI client — enabling mix-and-match architectures where any combination of Zoekt, SCIP, vector search, and graph traversal can serve any MCP-compatible agent.