# Contributing to WASM-operator

## Code quality

This project employs several formatters and linters to ensure code consistency and maintain high-quality standards.  
Contributors are expected to adhere to these practices and use the tools provided.

| Language | Formatter / Linter | Command |
| -------- | ------------------ | ------- |
| Rust     | [rustfmt](https://github.com/rust-lang/rustfmt) (F) <br> [clippy](https://github.com/rust-lang/rust-clippy) (L)  | `cargo fmt --all` <br> `cargo clippy --all` |
| Go       | [gofmt](https://pkg.go.dev/cmd/gofmt) (F)              | `go fmt` |
| Shell    | [shfmt](https://github.com/mvdan/sh#shfmt) (F) | `shfmt` |
| Python   | [Ruff](https://github.com/astral-sh/ruff) (F+L) | `ruff format` <br> `ruff check` |
| Markdown | [markdownlint](https://github.com/DavidAnson/markdownlint) (L) | `markdownlint '**/*.md'` |

> [!TIP]
> These can be setup using VSCode as well
>
> - Rust: [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) with settings.json: `"rust-analyzer.check.command": "clippy"`
> - Go: [Go](https://marketplace.visualstudio.com/items?itemName=golang.Go)
> - Shell: [Shell-format](https://marketplace.visualstudio.com/items?itemName=foxundermoon.shell-format)
> - Python: [Ruff](https://marketplace.visualstudio.com/items?itemName=charliermarsh.ruff)
> - Markdown: [markdownlint](https://marketplace.visualstudio.com/items?itemName=DavidAnson.vscode-markdownlint)
