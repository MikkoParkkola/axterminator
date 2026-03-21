# AXTerminator Release Checklist

## Pre-release Verification

- [ ] Version bumped in `Cargo.toml`
- [ ] Version bumped in `pyproject.toml`
- [ ] CHANGELOG.md updated with new version entry
- [ ] `cargo test` passes
- [ ] `cargo clippy --all-features` clean (0 warnings)
- [ ] `cargo audit` clean (0 advisories)
- [ ] Python type stubs present at `python/axterminator/__init__.pyi`

## Build and Test

- [ ] `cargo build --release --features cli` succeeds
- [ ] `cargo build --release --features "cli,audio,camera,spaces"` succeeds
- [ ] `maturin build --release` succeeds (Python wheel)
- [ ] Test install in fresh venv:
  ```bash
  python -m venv /tmp/ax-test && source /tmp/ax-test/bin/activate
  pip install target/wheels/axterminator-*.whl
  python -c "import axterminator as ax; print(ax.__version__)"
  ```
- [ ] Verify type stubs work in IDE (mypy, pyright)
- [ ] Run tests: `pytest python/tests/ -m "not requires_app" -v`

## Publish (4-Channel Pipeline)

### 1. crates.io
- [ ] `cargo publish`
- [ ] Verify: https://crates.io/crates/axterminator

### 2. PyPI
- [ ] `maturin publish --username __token__ --password $PYPI_TOKEN`
- [ ] Verify: https://pypi.org/project/axterminator/

### 3. GitHub Release
- [ ] Tag: `git tag -a vX.Y.Z -m "Release vX.Y.Z: ..."`
- [ ] Push tag: `git push origin vX.Y.Z`
- [ ] Create GitHub Release with CHANGELOG entry and binary assets
- [ ] Verify binary download works

### 4. Homebrew
- [ ] Homebrew formula auto-updated on tag push (CI)
- [ ] Verify: `brew install MikkoParkkola/tap/axterminator`

## Post-release

- [ ] MkDocs site updated: `mkdocs gh-deploy`
- [ ] Verify site: https://mikkoparkkola.github.io/axterminator/

## Verification Matrix

| Target | Python | Format |
|--------|--------|--------|
| macOS arm64 | 3.9-3.14 | abi3 wheel |
| macOS x86_64 | 3.9-3.14 | abi3 wheel (cross-compiled) |
