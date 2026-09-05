default:
    @just --list

check:
    cargo check --workspace --all-targets

test:
    cargo test --workspace

fmt:
    cargo fmt --all

# Same flags as CI, so a lint that fails there fails here first.
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

precommit: fmt check clippy test

# Rewrite a crate's CHANGELOG.md from git history. cargo-release runs this for
# every crate before a release, passing the crate and version in its environment
changelog crate=env("CRATE_NAME", "") version=env("NEW_VERSION", ""):
    #!/usr/bin/env sh
    set -eu
    case "{{ crate }}" in
        busybar-anim) dir="crates/busybar-anim"; set -- --include-path "$dir/**" ;;
        busyshow)     dir="."; set -- --exclude-path "crates/**" ;;
        *) echo "changelog: crate must be busybar-anim or busyshow" >&2; exit 1 ;;
    esac
    if [ -n "{{ version }}" ]; then
        set -- "$@" --tag "v{{ version }}"
    fi
    if [ "${DRY_RUN:-false}" = "true" ]; then
        git cliff "$@"
    else
        git cliff "$@" --output "$dir/CHANGELOG.md"
    fi

# Dry run of a release; add `--execute` yourself once it looks right.
release level="patch" *args: precommit
    cargo release {{ level }} --workspace {{ args }}
