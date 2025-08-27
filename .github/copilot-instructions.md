# Copilot Project Instructions for RISICO-2023

## 1. Overview

This file enables AI coding assistants to generate features that align with the RISICO-2023 project's architecture and style. All guidance is based strictly on observed, project-specific patterns—no invented or external best practices are included.

## 2. File Category Reference

Below are the file categories, their purpose, representative examples, and unique conventions:

### cli-entrypoints
- **Purpose:** Command-line binaries for running, converting, or testing the wildfire risk model.
- **Examples:** `src/bin/main.rs`, `src/bin/converter.rs`
- **Conventions:** Use the `clap` crate for argument parsing; keep logic minimal and delegate to the core library; include author/version metadata.

### core-library
- **Purpose:** Main library code, re-exporting all modules and providing shared constants and version info.
- **Examples:** `src/lib/mod.rs`, `src/lib/constants.rs`
- **Conventions:** Modular structure; only expose what is needed; add new logic as modules and re-export in `mod.rs`.

### model-logic
- **Purpose:** Data types and logic for model inputs/outputs.
- **Examples:** `src/lib/models/input.rs`, `src/lib/models/output.rs`
- **Conventions:** Use Rust structs with doc comments; provide `Default` implementations; use `serde` for serialization.

### modules
- **Purpose:** Shared logic and utilities, including fire index modules.
- **Examples:** `src/lib/modules/mod.rs`, `src/lib/modules/functions.rs`
- **Conventions:** Each module in its own directory with `mod.rs`; shared functions in `functions.rs`; all modules re-exported in `modules/mod.rs`.

### fire-index-modules
- **Purpose:** Implementation of each fire risk index as a self-contained module.
- **Examples:** `src/lib/modules/risico/`, `src/lib/modules/fwi/`
- **Conventions:** Each index has `mod.rs`, `functions.rs`, `models.rs`, `constants.rs`; new indices follow this structure.

### bin-common
- **Purpose:** Shared CLI logic and helpers.
- **Examples:** `src/bin/common/mod.rs`, `src/bin/common/helpers.rs`
- **Conventions:** Helpers in `helpers.rs`; all code intended for use by multiple binaries.

### bin-common-config
- **Purpose:** Configuration logic for CLI binaries.
- **Examples:** `src/bin/common/config/builder.rs`, `src/bin/common/config/data.rs`
- **Conventions:** Use builder pattern; data structures in `data.rs`/`models.rs`; tests in `test.rs`.

### bin-common-io
- **Purpose:** I/O logic for CLI binaries.
- **Examples:** `src/bin/common/io/mod.rs`, `src/bin/common/io/writers.rs`
- **Conventions:** Separate I/O from model/config logic; files named after function.

### bin-common-io-models
- **Purpose:** Data models for I/O.
- **Examples:** `src/bin/common/io/models/data.rs`, `src/bin/common/io/models/output.rs`
- **Conventions:** Each file is a specific data structure; entrypoint is `mod.rs`.

### bin-common-io-readers
- **Purpose:** Input readers for various formats.
- **Examples:** `src/bin/common/io/readers/binary.rs`, `src/bin/common/io/readers/netcdf.rs`
- **Conventions:** Each file implements a format; entrypoint is `mod.rs`; shared traits in `prelude.rs`.

### configuration
- **Purpose:** Main configuration file for the model.
- **Examples:** `configuration.yml`
- **Conventions:** All values documented and structured; used by CLI and core library.

### build-scripts
- **Purpose:** Build and release automation.
- **Examples:** `build-with-docker.sh`, `Dockerfile`
- **Conventions:** Scripts at project root; named after function; Dockerfile for container builds.

### ci-cd
- **Purpose:** Continuous integration and deployment workflows.
- **Examples:** `.github/workflows/build-on-release.yml`
- **Conventions:** Workflows named after trigger/function; documented with comments.

### documentation
- **Purpose:** Project documentation and licensing.
- **Examples:** `README.md`, `LICENSE.md`
- **Conventions:** Clear, concise, and focused on wildfire risk modeling; code examples reflect actual usage.

## 3. Feature Scaffold Guide

- **Determine file categories:** Use the above reference to decide which categories are needed for your feature (e.g., new fire index, new CLI tool, new I/O format).
- **File placement:** Place new files in the appropriate directory (e.g., new fire index in `src/lib/modules/`, new CLI in `src/bin/`).
- **Naming and structure:** Follow the naming and structure conventions for each category. For a new fire index, create a directory with `mod.rs`, `functions.rs`, `models.rs`, and `constants.rs`.
- **Example:** To add a new fire index "XIndex":
  - Create `src/lib/modules/xindex/` with the required files.
  - Add `pub mod xindex;` to `src/lib/modules/mod.rs`.
  - Add logic and types following the patterns in other indices.

## 4. Integration Rules

- All CLI logic must be in `src/bin/` and delegate to the core library.
- All new model logic or fire index implementations must be added as modules under `src/lib/modules/`.
- All data reading/writing logic must be placed in `src/bin/common/io/` and use the models/readers/writers structure.
- All configuration-related code must be placed in `src/bin/common/config/` and follow the builder/data/models/test modular pattern.
- Each fire index must follow the modular pattern: `mod.rs`, `functions.rs`, `models.rs`, `constants.rs`.

## 5. Example Prompt Usage

> "Add a new fire index called XIndex that calculates a custom risk metric."

Copilot would respond with:
- `src/lib/modules/xindex/mod.rs`
- `src/lib/modules/xindex/functions.rs`
- `src/lib/modules/xindex/models.rs`
- `src/lib/modules/xindex/constants.rs`
- Update to `src/lib/modules/mod.rs` to include `pub mod xindex;`

All files and logic would follow the conventions and structure described above.
