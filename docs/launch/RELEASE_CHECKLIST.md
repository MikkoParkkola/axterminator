# AXTerminator v0.3.2 Release Checklist

## Pre-release Verification

- [ ] Version bumped in `Cargo.toml` (currently `0.3.2`)
- [ ] Version bumped in `pyproject.toml` (currently `0.3.2`)
- [ ] CHANGELOG.md updated with v0.3.2 entry
- [ ] `cargo test` passes (37 passed, 0 failed, 3 ignored)
- [ ] `cargo clippy` clean (0 warnings)
- [ ] Python type stubs present at `python/axterminator/__init__.pyi`
- [ ] All `#[allow(dead_code)]` annotations documented in CHANGELOG

## Build and Test

- [ ] `maturin build --release` succeeds (test wheel creation)
- [ ] Test install in fresh venv:
  ```bash
  python -m venv /tmp/ax-test && source /tmp/ax-test/bin/activate
  pip install target/wheels/axterminator-0.3.2-*.whl
  python -c "import axterminator as ax; print(ax.__version__)"
  ```
- [ ] Verify type stubs work in IDE (mypy, pyright, VSCode autocomplete)
- [ ] Run integration tests with real macOS app (Calculator):
  ```bash
  pytest python/tests/ -m "not requires_app" -v
  ```

## Publish

- [ ] `maturin publish` to PyPI
  ```bash
  maturin publish --username __token__ --password $PYPI_TOKEN
  ```
- [ ] Verify on PyPI: https://pypi.org/project/axterminator/0.3.2/
- [ ] `pip install axterminator==0.3.2` works from PyPI

## Post-release

- [ ] GitHub release tag:
  ```bash
  git tag -a v0.3.2 -m "Release v0.3.2: Clippy cleanup, type stubs, doc improvements"
  git push origin v0.3.2
  ```
- [ ] Create GitHub Release with CHANGELOG entry as body
- [ ] Blog post draft reviewed and published (see `docs/launch/blog-post-draft.md`)
- [ ] Post to r/rust, r/python, Hacker News
- [ ] Update MkDocs site: https://mikkoparkkola.github.io/axterminator/

## Verification Matrix

| Target | Python | Status |
|--------|--------|--------|
| macOS arm64 | 3.9-3.14 | abi3 wheel |
| macOS x86_64 | 3.9-3.14 | abi3 wheel (cross-compiled) |
