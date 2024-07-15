init:
    cargo binstall taplo-cli cargo-nextest

test:
    cargo nextest run --all-features
    cargo test --doc --all-features

test-miri:
    cargo +nightly miri nextest run --all-features

lint:
    taplo lint
    cargo clippy --all-features

lint-fix:
    cargo clippy --fix --allow-staged
    taplo fmt
    cargo fmt --fix

doc:
    RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --document-private-items

ready:
    cargo fmt --check
    cargo clippy --no-deps
    cargo clippy --no-deps --features serde,nohash-hasher
    cargo clippy --no-deps --features atom_size_128
    cargo clippy --no-deps --features atom_size_64
    cargo clippy --no-deps --features atom_size_32
