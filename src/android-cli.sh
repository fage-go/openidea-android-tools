android_cli_path() {
    cli=$(command -v android || :)
    if [ -z "$cli" ]; then
        cli=${HOME:+$HOME/.local/bin/android}
    fi
    [ -n "$cli" ] && [ -x "$cli" ] || return 1
    printf '%s\n' "$cli"
}
