#!/usr/bin/env bash
# Run from the workspace root. Feature unification must not change public money
# to floats/numbers; a workspace-default run alone cannot detect that regression.
# Replay must also read those strings: serde-bincode implies serde-str, whose
# combination with serde-float changes Decimal's default text deserializer.
set -euo pipefail

for features in \
    '' \
    'rust_decimal/serde-float' \
    'rust_decimal/serde-arbitrary-precision' \
    'rust_decimal/serde-float,rust_decimal/serde-arbitrary-precision' \
    'rust_decimal/serde-str,rust_decimal/serde-float' \
    'rust_decimal/serde-bincode,rust_decimal/serde-float' \
    'rust_decimal/serde-str,rust_decimal/serde-float,rust_decimal/serde-arbitrary-precision' \
    'rust_decimal/serde-bincode,rust_decimal/serde-float,rust_decimal/serde-arbitrary-precision'; do
    args=()
    if [[ -n "$features" ]]; then args=(--features "$features"); fi
    cargo tree --locked -p szamlazz-agent -e features -i rust_decimal "${args[@]}"
    cargo test --locked -p szamlazz-agent --test decimal_serde --test numeric_fidelity "${args[@]}"
    cargo test --locked -p szamlazz-cli --test boundary "${args[@]}"
    cargo test --locked -p szamlazz-ipn --features serde --lib "${args[@]}"
    cargo test --locked -p szamlazz-adatkapcsolat --test monetary_serialization "${args[@]}"
    cargo test --locked -p restate-szamlazz --features schemars --test decimal_input "${args[@]}"
    cargo test --locked -p restate-szamlazz --lib service::journal:: "${args[@]}"
done
