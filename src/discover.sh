init=$1
cache=$2
mkdir -p .gradle
work=$(mktemp -d .gradle/openidea-android-tools.XXXXXXXX)
trap 'rm -rf "$work"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
printf '%s\n' "$init" > "$work/application-modules.gradle"
run_gradle --console=plain -q -Dorg.gradle.configuration-cache=false \
    "-DopenideaAndroidModules=$PWD/$work/modules" -I "$work/application-modules.gradle" help
[ -f "$work/modules" ] || fail 'Gradle 未返回应用模块列表'
mv -f "$work/modules" "$cache"
