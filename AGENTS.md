# Repository Guidelines

## Project Structure & Module Organization
Core crate metadata lives in `Cargo.toml`, while the primary domain logic is in `src/lib`, split into `models` and `modules` so each wildfire index (for example `src/lib/modules/risico`) stays isolated and testable. Executables reside in `src/bin`; `main.rs` drives the end-to-end forecast, `converter.rs` handles data conversion helpers, and shared helpers sit under `src/bin/common`. Configuration defaults are stored in `configuration.yml`, and model outputs produced during runs should be written to or inspected under `results/`.

## Build, Test, and Development Commands
Use `cargo build` for the standard debug build and `cargo build --release` when you need optimized binaries for production runs. Run the full model with `cargo run --bin main -- --config configuration.yml`, and explore converter utilities with `cargo run --bin converter -- --help`. Generate API documentation locally using `cargo doc --no-deps --open` to confirm public items stay well described.

## Coding Style & Naming Conventions
Follow Rust defaults: four-space indentation, snake_case for functions and modules, UpperCamelCase for types, and SHOUTY_SNAKE_CASE for constants (see `src/lib/constants.rs`). Format code before submitting with `cargo fmt`, and use `cargo clippy --all-targets --all-features` to catch lint regressions; address warnings or justify them with inline `allow` attributes sparingly. Keep module boundaries clean by re-exporting through `mod.rs` files instead of deep relative paths.

## Testing Guidelines
Unit and integration tests should sit alongside the code behind `#[cfg(test)]` modules; mirror the module name (e.g., `risico/tests::`) and prefer descriptive test names such as `should_compute_fire_danger`. Run `cargo test --all-features` before opening a pull request and add focused tests for new scenarios, especially around numerical thresholds or state transitions. When adding data-driven checks, commit lightweight fixtures and document expected ranges in test comments.

## Commit & Pull Request Guidelines
Adopt Conventional Commit summaries (`fix(snow): adjust ROS threshold`, `chore: update docs`), matching the existing history, and keep the subject under 72 characters. Each pull request should describe the change, reference related issues, list validation commands, and attach relevant output snippets or screenshots for result regressions. Ensure CI passes locally, request review from a domain maintainer familiar with the affected module, and be ready to clarify performance or accuracy impacts.
