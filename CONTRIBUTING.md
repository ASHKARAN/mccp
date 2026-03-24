# Contributing to MCCP

We welcome contributions to the Multi-Context Code Processor (MCCP) project! This guide will help you get started with development and understand our contribution process.

## Development Setup

### Prerequisites

- Rust 1.70+ and Cargo
- Git
- Basic knowledge of Rust and async programming

### Building the Project

1. Clone the repository:
   ```bash
   git clone https://github.com/your-org/mccp.git
   cd mccp
   ```

2. Build all components:
   ```bash
   cargo build
   ```

3. Run tests:
   ```bash
   cargo test
   ```

4. Build with optimizations:
   ```bash
   cargo build --release
   ```

### Project Structure

The project is organized as a workspace with the following crates:

- `mccp-core`: Core data structures and types
- `mccp-indexer`: Indexing pipeline and parsing logic
- `mccp-server`: MCP server implementation
- `mccp-storage`: Storage and caching layer
- `mccp-providers`: LLM provider abstractions
- `mccp-cli`: Command-line interface

## Code Style

We follow Rust's standard formatting and style guidelines:

- Use `cargo fmt` to format code
- Use `cargo clippy` to check for common issues
- Follow Rust naming conventions (snake_case for functions, PascalCase for types)
- Add documentation comments for public APIs

## Testing

We use unit tests and integration tests to ensure code quality:

```bash
# Run all tests
cargo test

# Run tests for a specific crate
cargo test --package mccp-core

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_function_name
```

## Adding New Features

### Adding Support for a New Programming Language

1. Add the language to the `Language` enum in `mccp-core/src/lib.rs`:
   ```rust
   #[derive(Debug, Clone, PartialEq, Eq, Hash)]
   pub enum Language {
       // ... existing languages
       NewLanguage,
   }
   ```

2. Add file extension mapping in the `from_extension` method:
   ```rust
   impl Language {
       pub fn from_extension(ext: &str) -> Option<Self> {
           match ext.to_lowercase().as_str() {
               // ... existing mappings
               "newext" => Some(Language::NewLanguage),
               _ => None,
           }
       }
   }
   ```

3. Implement parsing logic in `mccp-indexer/src/parser.rs`:
   ```rust
   impl Parser {
       pub fn parse_new_language(&self, content: &str) -> Result<Vec<Symbol>> {
           // Implement parsing logic
           Ok(vec![])
       }
   }
   ```

4. Update the main parsing method to handle the new language:
   ```rust
   impl Parser {
       pub fn parse(&self, content: &str, language: Language) -> Result<Vec<Symbol>> {
           match language {
               // ... existing languages
               Language::NewLanguage => self.parse_new_language(content),
           }
       }
   }
   ```

### Adding a New LLM Provider

1. Implement the `LlmProvider` trait in `mccp-providers/src/providers.rs`:
   ```rust
   #[derive(Debug, Clone)]
   pub struct NewProvider {
       // Provider-specific fields
   }

   #[async_trait::async_trait]
   impl LlmProvider for NewProvider {
       async fn complete(&self, prompt: &str, schema: Option<&JsonSchema>) -> Result<String> {
           // Implement completion logic
       }

       async fn stream(&self, prompt: &str) -> Result<tokio::sync::mpsc::Receiver<String>> {
           // Implement streaming logic
       }

       async fn health(&self) -> ProviderHealth {
           // Implement health check
       }

       fn id(&self) -> String {
           "new-provider".to_string()
       }

       fn name(&self) -> String {
           "New Provider".to_string()
       }

       fn version(&self) -> String {
           "1.0.0".to_string()
       }

       fn models(&self) -> Vec<String> {
           vec!["new-model".to_string()]
       }
   }
   ```

2. Add the provider to the provider manager tests

3. Update configuration support in `mccp-providers/src/config.rs`

### Adding New CLI Commands

1. Add the command to the `Commands` enum in `mccp-cli/src/commands.rs`:
   ```rust
   #[derive(Subcommand)]
   enum Commands {
       // ... existing commands
       NewCommand(NewCommand),
   }
   ```

2. Create the command struct:
   ```rust
   pub struct NewCommand {
       // Command arguments
   }

   #[async_trait::async_trait]
   impl Command for NewCommand {
       async fn execute(&self, config: &CliConfig) -> anyhow::Result<()> {
           // Implement command logic
       }
   }
   ```

3. Add command parsing to the main CLI parser

## Pull Request Process

1. **Fork the repository** and create a feature branch:
   ```bash
   git checkout -b feature/your-feature-name
   ```

2. **Make your changes** following our coding standards

3. **Add tests** for your changes:
   ```bash
   cargo test
   ```

4. **Update documentation** if needed (README.md, inline docs)

5. **Run the full test suite**:
   ```bash
   cargo test --all
   ```

6. **Commit your changes** with a clear commit message:
   ```bash
   git commit -m "Add support for new language"
   ```

7. **Push to your fork**:
   ```bash
   git push origin feature/your-feature-name
   ```

8. **Create a Pull Request** with:
   - Clear title and description
   - Reference any related issues
   - Describe the changes and motivation
   - Include any breaking changes

## Issue Reporting

When reporting issues, please include:

1. **Clear title** describing the problem
2. **Detailed description** of the issue
3. **Steps to reproduce** the problem
4. **Expected behavior** vs **actual behavior**
5. **Environment information** (OS, Rust version, etc.)
6. **Relevant logs or error messages**

## Code Review Process

- All submissions require review by at least one maintainer
- We aim to review pull requests within 3-5 business days
- Be open to feedback and willing to make changes
- Address all review comments before merge

## Development Guidelines

### Performance

- Consider performance implications of your changes
- Use async/await for I/O operations
- Avoid unnecessary allocations
- Profile performance-critical code

### Error Handling

- Use the `anyhow` crate for error handling
- Provide clear, actionable error messages
- Handle errors gracefully where possible
- Use appropriate error types

### Testing

- Write unit tests for new functionality
- Add integration tests for complex features
- Test edge cases and error conditions
- Ensure tests are fast and reliable

### Documentation

- Document public APIs with clear examples
- Update README.md for major features
- Add inline comments for complex logic
- Keep documentation up-to-date

## Getting Help

- Check existing issues and discussions
- Ask questions in our Discord server
- Create a new issue for bugs or feature requests
- Be patient and respectful in all interactions

## Code of Conduct

We expect all contributors to follow our Code of Conduct:
- Be respectful and inclusive
- Focus on what is best for the community
- Show empathy towards other community members
- Be open to constructive criticism

## Security

- Report security vulnerabilities privately
- Do not commit secrets or keys to the repository
- Follow secure coding practices
- Review dependencies for security issues

Thank you for contributing to MCCP! 🚀