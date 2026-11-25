set -eu

fail() {
    printf '%s\n' "$*" >&2
    exit 1
}

jvm=$1
shift
if [ -n "$jvm" ]; then
    case "$jvm" in
        '~/'*) jvm=$HOME/${jvm#\~/} ;;
        /*) ;;
        *) jvm=$PWD/$jvm ;;
    esac
    [ -x "$jvm/bin/java" ] || fail "项目配置的 Gradle JVM 不可用 $jvm"
    export JAVA_HOME="$jvm"
fi

run_gradle() {
    if [ -x ./gradlew ]; then
        ./gradlew "$@"
    elif [ -f ./gradlew ]; then
        sh ./gradlew "$@"
    else
        gradle "$@"
    fi
}
