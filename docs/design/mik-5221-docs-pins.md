# MIK-5221: pin the docs pip toolchain

No implementation in this document.

## §P0

**FOR:** Dependabot covers pip as well as cargo; the docs workflow installs from a pinned `docs/requirements.txt` that floors the eight flagged packages; openssl stays >= 0.10.79.

**OUT:** Rewriting the mkdocs site; bumping openssl past 0.10.80; closing HIGH alerts that are already 0.

## Named decisions

- Manifest lives at `docs/requirements.txt` (the only pip surface) with `directory: "/docs"` in dependabot.
- Entrypoints stay named; vulnerable transitives are explicit `>=` floors so pip-audit can fail the tree.
- AC.3 is a Cargo.lock regex guard, not a crate bump (already 0.10.80).
