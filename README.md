# MCCP - Multi-Context Code Processor

MCCP is a comprehensive tool for indexing and analyzing codebases to provide rich context for Large Language Models (LLMs). It's designed to help LLMs understand code structure, relationships, and context to improve code generation and analysis capabilities.

## Features

- **Multi-language Support**: Supports Rust, TypeScript, JavaScript, Python, Go, Java, C/C++, C#, Ruby, PHP, Swift, Kotlin, and more
- **Intelligent Indexing**: Parses source files to extract symbols, functions, classes, and other code elements
- **Smart Chunking**: Splits code into meaningful chunks with configurable size and overlap
- **LLM Integration**: Provides structured context and summaries for LLMs
- **Call Graph Analysis**: Builds dependency graphs to understand code relationships
- **Real-time Updates**: Watches files for changes and updates indexes automatically
- **MCP Server**: Provides a Model Context Protocol server for integration with LLM tools
- **CLI Interface**: Command-line tools for project management and analysis

## Architecture

MCCP is built as a modular system with the following components:

### Core Components

- **mccp-core**: Core data structures and types
- **mccp-indexer**: Indexing pipeline with parsing, chunking, and summarization
- **mccp-server**: MCP server for providing context to LLMs
- **mccp-storage**: Storage layer with caching and persistence
- **mccp-providers**: Provider abstraction for different LLM services
- **mccp-cli**: Command-line interface

### Key Concepts

- **Projects**: Represent codebases with configuration for indexing
- **Symbols**: Code elements like functions, classes, variables, etc.
- **Chunks**: Segments of code with metadata for LLM processing
- **Summaries**: LLM-generated descriptions of code chunks
- **Graphs**: Call graphs and dependency relationships

## Installation

### Prerequisites

- Rust 1.70+
- Cargo

### Building from Source

```bash
git clone https://github.com/your-org/mccp.git
cd mccp
cargo build --release
```

### Installation

```bash
cargo install --path .
```

## Usage

### Quick Start

1. **Initialize a project**:
   ```bash
   mccp init --path /path/to/project --name my-project --language rust
   ```

2. **Start the daemon**:
   ```bash
   mccp start
   ```

3. **Index your project**:
   ```bash
   mccp index --path /path/to/project
   ```

4. **Use with LLMs**:
   The MCP server will be available at `http://localhost:3000` for LLM tools to connect to.

### CLI Commands

#### Project Management

```bash
# Initialize a new project
mccp init --path /path/to/project --name project-name --language rust

# Get project information
mccp project --path /path/to/project --detailed
```

#### Indexing

```bash
# Index a project
mccp index --path /path/to/project

# Force re-indexing
mccp index --path /path/to/project --force

# Verbose indexing
mccp index --path /path/to/project --verbose
```

#### Searching

```bash
# Search for symbols
mccp search --path /path/to/project --query "function_name" --search-type symbols

# Search for code chunks
mccp search --path /path/to/project --query "error handling" --search-type chunks

# Limit results
mccp search --path /path/to/project --query "test" --limit 10
```

#### Provider Management

```bash
# List available providers
mccp provider --list

# Test a provider
mccp provider --test openai

# Add a provider (requires configuration)
mccp provider --add openai
```

#### Statistics

```bash
# Get project statistics
mccp stats --path /path/to/project --detailed

# Get system statistics
mccp stats --detailed
```

#### Configuration

```bash
# Show current configuration
mccp config --show

# Set configuration value
mccp config --set "server.port=8080"

# Reset to defaults
mccp config --reset
```

#### Testing

```bash
# Test all components
mccp test --all

# Test specific component
mccp test --component core
mccp test --component indexer
mccp test --component storage
mccp test --component providers
```

### Daemon Usage

```bash
# Start the daemon
mccp daemon --port 3000 --host 0.0.0.0

# Start without waiting
mccp daemon --no-wait

# Stop the daemon
mccp daemon stop
```

## Configuration

### Global Configuration

MCCP uses a configuration file located at `~/.mccp/config.toml`:

```toml
[server]
port = 3000
host = "127.0.0.1"
max_connections = 100
enable_caching = true
cache_ttl = 3600

[indexer]
watch_enabled = true
parallel_workers = 0
include_patterns = ["**/*"]
exclude_patterns = [
    "**/node_modules/**",
    "**/.git/**",
    "**/target/**",
    "**/build/**"
]

[storage]
backend = "memory"
path = "/tmp/mccp_storage"
enable_compression = false
compression_level = 1

[providers]
default = "local"
settings = {}
```

### Project Configuration

Each project has its own configuration file at `.mccp.toml`:

```toml
project_id = "my-project"
root_path = "/path/to/project"
language = "rust"
include_patterns = ["**/*.rs"]
exclude_patterns = ["**/tests/**", "**/examples/**"]
chunk_size = 512
chunk_overlap = 64
watch_enabled = true
```

## API Reference

### MCP Server Endpoints

The MCP server provides the following endpoints:

- `GET /projects` - List all projects
- `GET /projects/{id}` - Get project details
- `GET /projects/{id}/symbols` - Get symbols for a project
- `GET /projects/{id}/chunks` - Get chunks for a project
- `GET /projects/{id}/summaries` - Get summaries for a project
- `GET /projects/{id}/graph` - Get call graph for a project
- `GET /search/{id}?q=query` - Search symbols and chunks
- `GET /context/{id}/{file}/{line}/{column}` - Get context for a location
- `GET /stats/{id}` - Get project statistics

### CLI API

The CLI provides a comprehensive set of commands for managing projects, indexing, searching, and system administration.

## Integration with LLMs

MCCP integrates with LLMs through the Model Context Protocol (MCP). LLM tools can connect to the MCP server to:

- Retrieve project structure and symbols
- Get code context for specific locations
- Search for relevant code snippets
- Access call graphs and dependencies
- Get LLM-generated summaries of code chunks

### Supported LLM Providers

- **OpenAI**: GPT-4, GPT-3.5-turbo
- **Anthropic**: Claude 3 models
- **Local**: Mock provider for testing

## Development

### Building

```bash
# Build all components
cargo build

# Build with optimizations
cargo build --release

# Run tests
cargo test

# Run specific tests
cargo test --package mccp-core
cargo test --package mccp-indexer
```

### Adding New Languages

To add support for a new programming language:

1. Add the language to the `Language` enum in `mccp-core`
2. Implement parsing logic in `mccp-indexer`
3. Add file extension mappings
4. Update chunking strategies if needed

### Adding New LLM Providers

To add a new LLM provider:

1. Implement the `LlmProvider` trait
2. Add the provider to `mccp-providers`
3. Update the provider manager
4. Add configuration support

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests for your changes
5. Run the test suite: `cargo test`
6. Submit a pull request

## License

This project is licensed under the MIT License.

## Support

For support and questions:

- Create an issue on GitHub
- Join our Discord server
- Email us at support@example.com

## Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.