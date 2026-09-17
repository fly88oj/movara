<!-- Thank you! Keep the summary short; link issues with "Fixes #N". -->

## Summary

<!-- What changed and why. One or two sentences. -->

Fixes #

## Type

<!-- check one (Conventional Commits types) -->
- [ ] feat — new functionality
- [ ] fix — bug fix
- [ ] docs — documentation only
- [ ] refactor / perf — no behaviour change
- [ ] test / build / ci / chore

## Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo test --all` passes (new behaviour is covered by tests)
- [ ] user-facing strings added to **all eight** `locales/*.yml` files
- [ ] `CHANGELOG.md` updated (Keep a Changelog format)
- [ ] for new adapters: README tables updated in all languages and the
      encoding algorithm is verified against real data (docs/research.md)
- [ ] no local machine info / credentials introduced
      (`scripts/check-no-local-info.sh` passes)
