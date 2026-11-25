require_avdmanager() {
    avdmanager=
    for directory in "$sdk"/cmdline-tools/latest/bin "$sdk"/cmdline-tools/*/bin "$sdk"/tools/bin; do
        if [ -x "$directory/avdmanager" ]; then
            avdmanager=$directory/avdmanager
            break
        fi
        if [ -f "$directory/avdmanager.bat" ]; then
            avdmanager=$directory/avdmanager.bat
            break
        fi
    done
    [ -n "$avdmanager" ] || fail '请在 Android SDK 中安装 SDK Command-line Tools（cmdline-tools;latest 包）。'
    if [ "$(uname -s)" = Darwin ]; then
        sdk_java=$(/usr/libexec/java_home -v '17+' 2>/dev/null || :)
        studio='/Applications/Android Studio.app/Contents/jbr/Contents/Home'
        if [ -z "$sdk_java" ] && [ -x "$studio/bin/java" ]; then sdk_java=$studio; fi
        if [ -n "$sdk_java" ]; then export JAVA_HOME="$sdk_java"; fi
    fi
}
