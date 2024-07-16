init:
    cargo binstall taplo-cli cargo-nextest

test:
    cargo nextest run
    cargo test --doc

test-miri:
    cargo +nightly miri nextest run

lint:
    taplo lint
    cargo clippy --features serde,nohash-hasher

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
