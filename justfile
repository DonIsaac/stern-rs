init:
    cargo binstall taplo-cli cargo-nextest

test:
    cargo nextest run
    cargo test --doc

miri:
    MIRIFLAGS=-Zmiri-backtrace=1 cargo +nightly miri nextest run --nocapture
    cargo +nightly miri test run

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

asm:
    cargo asm --target x86_64-unknown-linux-gnu --lib --rust --include-constants --color try_new_unchecked
asm-dev:
    cargo asm --target x86_64-unknown-linux-gnu --dev --lib --rust --include-constants --color try_new_unchecked 1
# x86_64-unknown-none
