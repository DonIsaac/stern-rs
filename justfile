lint:
    taplo lint
    cargo clippy

lint-fix:
    cargo clippy --fix --allow-staged -- -W pedantic
    taplo fmt
    cargo fmt --fix

doc:
    cargo doc --no-deps --document-private-items
