default: run

# Start Garden Relay locally. Usage: just run [port] [database]
run port="8080" database="gardenrelay.db":
    GARDEN_RELAY_PORT="{{port}}" GARDEN_RELAY_DATABASE_PATH="{{database}}" cargo run

# Start with the example policies loaded at boot.
run-with-policies port="8080" database="gardenrelay.db":
    GARDEN_RELAY_PORT="{{port}}" GARDEN_RELAY_DATABASE_PATH="{{database}}" GARDEN_RELAY_POLICY_DIR="examples/policies" cargo run

fmt:
    cargo fmt

fmt-check:
    cargo fmt --check

lint:
    cargo clippy --all-targets -- -D warnings

test:
    cargo test

check: fmt-check lint test
