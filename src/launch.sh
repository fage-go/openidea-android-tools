legacy_init=$1
module=$2
variant=$3
action=${4:-run}
preferred=
serial=
tab=$(printf '\t')
config=.gradle/openidea-android-tools.conf
if [ -f "$config" ]; then
    {
        IFS= read -r panel_module || :
        IFS= read -r panel_variant || :
        IFS= read -r serial || :
        if [ "$panel_module" = "$module" ]; then
            preferred=$panel_variant
        fi
        while IFS="$tab" read -r saved_module saved_variant; do
            if [ "$saved_module" = "$module" ]; then
                preferred=$saved_variant
                break
            fi
        done
    } < "$config"
fi
case "$module" in
    :*) ;;
    *) fail '请在 Android 工具窗口选择应用模块。' ;;
esac
variants=.gradle/openidea-android-tools.variants
if [ -r "$variants" ]; then
    variant=$(awk -F "$tab" -v module="$module" -v preferred="$preferred" -v fallback="$variant" '
        $1 == module {
            for (i = 2; i <= NF; i++) {
                if ($i == preferred && $i != "") { print $i; exit }
                if ($i == fallback) found = $i
            }
            if (found != "") print found
            exit
        }
    ' "$variants")
else
    variant=${preferred:-$variant}
fi
case "$variant" in ''|*[!a-zA-Z0-9_-]*) fail '请刷新配置并选择当前模块支持的构建变体。' ;; esac
case "$action" in build|install|run) ;; *) fail '未知的 Android 操作。' ;; esac

if [ "$action" != "build" ]; then
    [ -n "$serial" ] || fail '请在 Android 工具窗口选择部署设备。'
    adb_path=$(command -v adb || :)
    if [ -z "$adb_path" ]; then
        sdk=${ANDROID_HOME:-"$HOME/Library/Android/sdk"}
        adb_path=$sdk/platform-tools/adb
    fi
    [ -x "$adb_path" ] || fail '找不到 ADB，请检查 Android SDK。'
    state=$("$adb_path" -s "$serial" get-state 2>/dev/null || :)
    [ "$state" = "device" ] || fail "设备 $serial 未连接或未授权，请重新选择。"
fi

wrapper_config=gradle/wrapper/gradle-wrapper.properties
distribution=
if [ -r "$wrapper_config" ]; then
    distribution=$(sed -n '/^distributionUrl=/s/.*\(gradle-.*\)\.zip.*/\1/p' "$wrapper_config")
fi
legacy=false
case "$distribution" in
    gradle-[1-5].*|gradle-6.[0-7][.-]*) legacy=true ;;
esac

task_prefix=$module
[ "$module" = ":" ] || task_prefix=$module:
first=$(printf '%.1s' "$variant" | tr '[:lower:]' '[:upper:]')
task_variant=$first$(printf '%s' "$variant" | cut -c 2-)
assemble_task="${task_prefix}assemble$task_variant"
if [ "$action" = "build" ]; then
    run_gradle "$assemble_task"
    exit
fi

android_cli=$(android_cli_path) || fail '找不到 Android CLI，请检查 PATH 或 ~/.local/bin/android。'

if [ "$legacy" != "true" ]; then
    exec "$android_cli" "$action" "$module@$variant" "--device=$serial"
fi

tmp_dir=$(mktemp -d "${TMPDIR:-/tmp}/openidea-android.XXXXXXXX")
trap 'rm -rf "$tmp_dir"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
printf '%s\n' "$legacy_init" > "$tmp_dir/apks.gradle"

run_gradle "$assemble_task" -I "$tmp_dir/apks.gradle" \
    "-PopenideaAndroidModule=$module" "-PopenideaAndroidVariant=$variant" \
    "-PopenideaAndroidApkPath=$tmp_dir/apk-path"
[ -s "$tmp_dir/apk-path" ] || fail 'Gradle 未返回当前构建变体的 APK 路径。'
IFS= read -r apk < "$tmp_dir/apk-path"
[ -f "$apk" ] || fail '构建结束后没有找到 APK。'
"$android_cli" "$action" "--apks=$apk" "--device=$serial"
