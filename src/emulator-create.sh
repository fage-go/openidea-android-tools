require_emulator
require_avdmanager
name=$1
profile=$2
image=$3
orientation=$4
if [ ! -f "$sdk/$image/source.properties" ] && [ ! -f "$sdk/$image/package.xml" ]; then
    cli=$(android_cli_path) || fail '找不到 Android CLI，无法下载系统镜像。'
    printf '@download\n'
    "$cli" --sdk="$sdk" sdk install --ignore-outdated-xmls "$image" </dev/null 2>&1
fi
printf '\n@create\n'
package=$(printf '%s' "$image" | tr '/' ';')
printf 'no\n' | "$avdmanager" create avd -n "$name" -d "$profile" -k "$package"
if [ "$orientation" != default ]; then
    config=$(avd_config "$name") || fail '模拟器已创建，启动方向未保存。'
    tmp=$(mktemp "$config.XXXXXX")
    trap 'rm -f "$tmp"' EXIT
    awk -F= -v mode="$orientation" '
        { lines[NR] = $0 }
        $1=="hw.lcd.width" { w = $2+0 }
        $1=="hw.lcd.height" { h = $2+0 }
        END {
            swap = w && h && ((mode=="landscape" && w<h) || (mode=="portrait" && w>h))
            for (i=1; i<=NR; i++) {
                line=lines[i]
                if (line ~ /^hw[.]initialOrientation=/) continue
                if (swap && line ~ /^(hw[.]lcd[.](width|height)|skin[.](path|name))=/) continue
                print line
            }
            print "hw.initialOrientation=" mode
            if (swap) printf "hw.lcd.width=%d\nhw.lcd.height=%d\nskin.name=%dx%d\nskin.path=_no_skin\n", h,w,h,w
        }
    ' "$config" > "$tmp"
    mv "$tmp" "$config"
fi
