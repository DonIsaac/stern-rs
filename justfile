init:
    cargo binstall taplo-cli cargo-nextest

test:
    cargo nextest run --all-features
    cargo test --doc --all-features

lint:
    taplo lint
    cargo clippy --all-features

lint-fix:
    cargo clippy --fix --allow-staged
    taplo fmt
    cargo fmt --fix

doc:
    RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --document-private-items
