# Resolve both relative and absolute paths to one installed SDK package.
sdk_root=$(cd -P "$sdk" && pwd)
image_path() {
    path=$(printf '%s' "$1" | tr '\\' '/')
    case "$path" in /*|[A-Za-z]:/*) ;; *) path=$sdk/$path ;; esac
    (cd -P "$path" 2>/dev/null && pwd)
}

target=
if [ -n "$1" ]; then target=$(avd_config "$1") || :; fi
if [ -n "$target" ] && [ -e "${target%/*}/hardware-qemu.ini.lock" ]; then
    fail '请先停止模拟器。'
fi

if [ -n "$target" ]; then
    source=$(sed -n 's/^image\.sysdir\.1[[:space:]]*=[[:space:]]*//p' "$target" | tr -d '\r' | head -n 1)
else
    source=${2:-}
fi
[ -n "$source" ] || exit 0
root=$(image_path "$source") || exit 0
case "$root" in "$sdk_root"/system-images/*) ;; *) exit 0 ;; esac
[ -f "$root/package.xml" ] || [ -f "$root/source.properties" ] || exit 0
printf '%s\n' "${root#"$sdk_root"/}"

avd_homes | while IFS= read -r dir; do
    [ -n "$dir" ] || continue
    for ini in "$dir"/*.ini; do
        [ -r "$ini" ] || continue
        path=$(sed -n 's/^path=//p' "$ini" | tr -d '\r')
        config=$path/config.ini
        if [ -z "$path" ] || [ ! -f "$config" ]; then
            config=${ini%.ini}.avd/config.ini
        fi
        [ -r "$config" ] || continue
        [ -n "$target" ] && [ "$config" -ef "$target" ] && continue
        sed -n 's/^image\.sysdir\.[0-9][0-9]*[[:space:]]*=[[:space:]]*//p' "$config" | tr -d '\r' |
            while IFS= read -r path; do
                [ -n "$path" ] || continue
                used=$(image_path "$path") || continue
                if [ "$used" = "$root" ]; then
                    printf 'shared\n'
                    break
                fi
            done
    done
done
