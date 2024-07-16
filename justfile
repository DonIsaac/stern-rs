# Install tools. Requires `cargo-binstall`.
init:
    cargo binstall taplo-cli cargo-nextest

test:
    cargo nextest run
    cargo test --doc

# Run tests with miri UB detection
miri *ARGS='':
    MIRIFLAGS=-Zmiri-strict-provenance cargo +nightly miri nextest run --nocapture {{ARGS}}

# Check for lint violations
lint:
    taplo lint
    cargo clippy --features serde,nohash-hasher
    cargo fmt --check

# Fix lint violations. Worktree must be clean/staged.
lint-fix:
    cargo clippy --no-deps --fix --allow-staged
    taplo fmt
    cargo fmt

doc:
    RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --document-private-items

# CI checks
ready:
    cargo fmt --check
    cargo clippy --no-deps
    cargo clippy --no-deps --features serde,nohash-hasher
    cargo clippy --no-deps --features atom_size_128
    cargo clippy --no-deps --features atom_size_64
    cargo clippy --no-deps --features atom_size_32
    just test
