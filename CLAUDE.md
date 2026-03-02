# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when
working with code in this repository.

## Project Overview

**ledger-rs** is a Rust implementation of a double-entry accounting
system (similar to ledger-cli). It's a CLI tool for tracking financial
transactions, accounts, and balances from plain-text journal files.

## Common Commands

```bash
# Build
cargo build
cargo build --release

# Run all tests
cargo test


# Run the binary directly
./target/debug/ledger -f samples/investing.jornal bal
echo "..." | ./target/debug/ledger bal
```

## Architecture

### Valuation Methods

The `Valuation` enum selects how holdings are priced:
- `Quantity` — raw commodity amounts (no conversion)
- `Basis` — book value / cost basis (`--basis`)
- `Market` — current market price (`--market`)
- `Historical` — price at acquisition time (`--historical`)

### Trait Hierarchy (`ntypes.rs`)

Types implement layered traits to support polymorphic arithmetic:
- `Zero` → check if value is zero
- `Arithmetic` → add, subtract, sum
- `Basket` → collection of commodities
- `Valuable` → compute value under a `Valuation`
- `TsBasket` → time-series baskets (values at different dates)

### Testing

Integration tests live in `ledger/tests/ledger-cli/`. Each `.test` file contains:
1. Journal data (the input)
2. One or more `test <command> ... end test` blocks with expected output

The Rust test harness (`ledger-cli_test.rs`) discovers all `.test`
files and runs each through `python3 tests/ledger-cli/run-test.py`,
comparing actual vs. expected output. Tests can assert non-zero exit
codes with `test command -> 1`.

### CLI Commands

- **`balance` / `bal`** — hierarchical account balances with optional
  `--flat`, `--collapse`, `--depth N`, `--empty`, `--no-total`
- **`register` / `reg`** — chronological posting list

Global flags: `-f/--file`, `-b/--begin`, `-e/--end`, `--price-db`,
`--fmt {tty|json|lisp}`, `--market`, `--basis`, `--historical`,
`--quantity`
