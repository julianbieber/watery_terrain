# Use bash strict mode
set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

# Read active rust toolchain from mise for display/logging

# Shared env (same as CI)
RUSTFLAGS_BASE := "-Zshare-generics=y -Zthreads=0"
RUSTDOCFLAGS_BASE := "-Zshare-generics=y -Zthreads=0"
WASM_TARGET := "wasm32-unknown-unknown"

# Default: run everything
default:
    @just --list

# Install system libraries used by CI (Ubuntu/Debian)
deps:
	@sudo apt-get update
	@sudo apt-get install --no-install-recommends -y libasound2-dev libudev-dev libwayland-dev

# Format check
fmt:
	@env \
	RUSTFLAGS="{{RUSTFLAGS_BASE}}" \
	RUSTDOCFLAGS="{{RUSTDOCFLAGS_BASE}}" \
	cargo fmt --all -- --check

# Docs check
docs:
	@env \
	RUSTFLAGS="{{RUSTFLAGS_BASE}}" \
	RUSTDOCFLAGS="{{RUSTDOCFLAGS_BASE}}" \
	cargo doc --locked --workspace --profile ci --all-features --document-private-items --no-deps

# Clippy lints
clippy:
	@env \
	RUSTFLAGS="{{RUSTFLAGS_BASE}}" \
	RUSTDOCFLAGS="{{RUSTDOCFLAGS_BASE}}" \
	cargo clippy --locked --workspace --all-targets --profile ci --all-features

# Bevy lints (requires bevy_lint on PATH)
bevy-lints:
	@env \
	RUSTFLAGS="{{RUSTFLAGS_BASE}}" \
	RUSTDOCFLAGS="{{RUSTDOCFLAGS_BASE}}" \
	bevy_lint --locked --workspace --all-targets --profile ci --all-features

# Install Bevy linter via the Bevy CLI installer, then ensure bevy_lint exists
bevy-lint-install:
	@bevy lint install
	@command -v bevy_lint >/dev/null 2>&1 || { echo "bevy_lint not on PATH; ensure installer completed."; exit 1; }

# Tests with cranelift backend
test:
	cargo test --locked --workspace

# Web compilation check with getrandom wasm cfg injection
check-web:
	@env \
	RUSTFLAGS="{{RUSTFLAGS_BASE}} --cfg getrandom_backend=\"wasm_js\"" \
	RUSTDOCFLAGS="{{RUSTDOCFLAGS_BASE}}" \
	cargo check \
	  --config 'profile.web.inherits="dev"' \
	  --profile ci \
	  --target {{WASM_TARGET}}

# Point wgsl-analyzer at the WESL sources of the bevy version in Cargo.lock.
# src/wesl.toml depends on the packages this writes to .wesl-deps/; rerun after
# every bevy bump.
wesl-deps:
	#!/usr/bin/env python3
	import json, pathlib, shutil, subprocess
	out = pathlib.Path(".wesl-deps")
	shutil.rmtree(out, ignore_errors=True)
	meta = json.loads(subprocess.run(
	    ["cargo", "metadata", "--format-version", "1", "--locked"],
	    capture_output=True, check=True).stdout)
	crates = {}
	for package in meta["packages"]:
	    table = (package.get("metadata") or {}).get("wgsl-analyzer")
	    if table and package["name"].startswith("bevy_"):
	        crates[package["name"]] = (pathlib.Path(package["manifest_path"]).parent, table)
	for name, (crate_dir, table) in sorted(crates.items()):
	    shim = out / name
	    shim.mkdir(parents=True)
	    (shim / "src").symlink_to(crate_dir / table.get("root", "shaders"))
	    deps = [d for d in table.get("dependencies", {}) if d in crates or d == "constants"]
	    (shim / "wesl.toml").write_text(
	        f'edition = "{table.get("edition", "2026_pre")}"\nroot = "src"\n\n[dependencies]\n'
	        + "".join(d + ' = { path = "../' + d + '" }\n' for d in deps))
	    print(f"{name} -> {crate_dir}")
	constants = out / "constants" / "shaders"
	constants.mkdir(parents=True)
	(out / "constants" / "wesl.toml").write_text('edition = "2026_pre"\n')
	(constants / "package.wesl").write_text("\n".join(
	    f"const {name}: u32 = {value};" for name, value in [
	        ("MATERIAL_BIND_GROUP", 3),
        ("CELLS_PER_METER", 10),
        ("WORKGROUP_SIZE", 8),
        ("MAX_DISPLACEMENTS", 64),
        ("DRY_DEPTH_MM", 10),
	        ("SORTED_FRAGMENT_MAX_COUNT", 128),
	        ("SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS", 128),
	        ("MAX_DIRECTIONAL_LIGHTS", 128),
	        ("MAX_CASCADES_PER_LIGHT", 128),
	        ("WORLD_CACHE_SIZE", 128),
	        ("SHADOW_SAMPLES", 128),
	    ]) + "\n")

# Run everything in CI order
all: fmt docs clippy bevy-lints test check-web

# Clean
clean:
	@cargo clean

# Run native via Bevy CLI
run:
	@env \
	RUSTFLAGS="{{RUSTFLAGS_BASE}}" \
	RUSTDOCFLAGS="{{RUSTDOCFLAGS_BASE}}" \
	bevy run

# Run web via Bevy CLI
# Bevy CLI handles building to wasm32-unknown-unknown and serving locally.
run-web:
	@env \
	RUSTFLAGS="{{RUSTFLAGS_BASE}}" \
	RUSTDOCFLAGS="{{RUSTDOCFLAGS_BASE}}" \
	bevy run web
