set -eu

fail() {
    printf '%s\n' "$*" >&2
    exit 1
}

hint=$1
shift
sdk=
for candidate in "$hint" "${ANDROID_HOME:-}" "${ANDROID_SDK_ROOT:-}"     "${HOME:+$HOME/Library/Android/sdk}" "${HOME:+$HOME/Android/Sdk}"     "${LOCALAPPDATA:+$LOCALAPPDATA/Android/Sdk}"; do
    if [ -n "$candidate" ] && [ -d "$candidate" ]; then
        sdk=$candidate
        break
    fi
done
[ -n "$sdk" ] || fail '找不到 Android SDK，请检查项目 local.properties 或 ANDROID_HOME。'
export ANDROID_HOME="$sdk" ANDROID_SDK_ROOT="$sdk"
emulator=$sdk/emulator/emulator
if [ ! -x "$emulator" ] && [ -x "$emulator.exe" ]; then emulator=$emulator.exe; fi

require_emulator() {
    [ -x "$emulator" ] || fail '请在 Android SDK 中安装 Android Emulator（emulator 包）。'
}

avd_homes() {
    printf '%s\n' "${ANDROID_AVD_HOME:-}" "${ANDROID_USER_HOME:+$ANDROID_USER_HOME/avd}"         "${ANDROID_EMULATOR_HOME:+$ANDROID_EMULATOR_HOME/avd}"         "${ANDROID_SDK_HOME:+$ANDROID_SDK_HOME/.android/avd}" "${HOME:+$HOME/.android/avd}"
}

avd_config() {
    avd_homes | (
        while IFS= read -r dir; do
            [ -n "$dir" ] && [ -r "$dir/$1.ini" ] || continue
            path=$(sed -n 's/^path=//p' "$dir/$1.ini" | tr -d '\r')
            if [ -n "$path" ] && [ -f "$path/config.ini" ]; then
                printf '%s\n' "$path/config.ini"
                exit 0
            fi
            if [ -f "$dir/$1.avd/config.ini" ]; then
                printf '%s\n' "$dir/$1.avd/config.ini"
                exit 0
            fi
        done
        exit 1
    )
}
