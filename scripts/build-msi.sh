#!/usr/bin/env bash
# 用 wixl（msitools）在 Linux 上打包 Windows MSI，不需要 Windows 或 Wine
set -Eeuo pipefail
cd "$(dirname "$0")/.."

TARGET_DIR="${TARGET_DIR:-src-tauri/target/x86_64-pc-windows-gnu/release}"
EXE="$TARGET_DIR/starlit-foss.exe"
INCLUDE_MEDIA="${INCLUDE_MEDIA:-1}"
PRODUCT_NAME="${PRODUCT_NAME:-Starlit FOSS}"
MANUFACTURER="${MANUFACTURER:-zgyhim}"

need() { command -v "$1" >/dev/null || { echo "缺少命令：$1" >&2; exit 1; }; }
need wixl
need python3

[ -f "$EXE" ] || { echo "找不到 $EXE，请先运行 scripts/build-windows.sh" >&2; exit 1; }

VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version" 2>/dev/null || echo 0.1.0)"
mkdir -p dist
WXS="dist/starlit-msi.wxs"
OUT="dist/StarlitFOSS-$VERSION-x64.msi"

echo "exe:      $EXE"
echo "版本:     $VERSION"
echo "媒体包:   $([ "$INCLUDE_MEDIA" = 1 ] && echo 包含 || echo 跳过)"
echo "生成 wxs: $WXS"

python3 - "$EXE" "$VERSION" "$PRODUCT_NAME" "$MANUFACTURER" "$INCLUDE_MEDIA" "$WXS" <<'PY'
import sys, uuid
from pathlib import Path
from xml.sax.saxutils import escape, quoteattr

exe, version, product, manufacturer, include_media, wxs = sys.argv[1:7]
root = Path.cwd()
files = [(Path(exe).resolve(), "starlit-foss.exe")]

template = root / "resources" / "template"

# 人物目录名与 meta.json 里的名字不一致时，说明文件名编码损坏（重复的乱码目录），
# 这些目录既冗余又会让 wixl 崩溃，打包时跳过。
skip_dirs = []
chars = template / "persona" / "characters"
if chars.is_dir():
    import json
    for child in sorted(chars.iterdir()):
        if not child.is_dir():
            continue
        try:
            meta_name = json.loads((child / "meta.json").read_text(encoding="utf-8")).get("name")
        except Exception:
            meta_name = None
        if meta_name and meta_name != child.name and not child.name.isascii():
            skip_dirs.append(child)
            print(f"跳过编码异常的目录: {child.name!r}（meta 名称: {meta_name}）", file=sys.stderr)

for path in sorted(template.rglob("*")):
    if not path.is_file():
        continue
    if any(d in path.parents for d in skip_dirs):
        continue
    rel = path.relative_to(template).as_posix()
    files.append((path, "resources/template/" + rel))

media = root / "resources" / "media"
if media.is_dir():
    for path in sorted(media.iterdir()):
        if not path.is_file():
            continue
        if path.name.endswith(".tar.zst") and include_media != "1":
            continue
        files.append((path, "resources/media/" + path.name))

def guid(key):
    return "{" + str(uuid.uuid5(uuid.NAMESPACE_URL, "starlit-foss/" + key)).upper() + "}"

tree = {}
for source, rel in files:
    parts = rel.split("/")
    node = tree
    for part in parts[:-1]:
        node = node.setdefault(part, {})
    node.setdefault("__files__", []).append((source, parts[-1]))

counter = 0
components = []

def emit(node):
    global counter
    xml = ""
    for name in sorted(k for k in node if k != "__files__"):
        counter += 1
        child_id = f"Dir_{counter}"
        xml += f'<Directory Id="{child_id}" Name={quoteattr(name)}>' + emit(node[name]) + "</Directory>"
    for source, filename in node.get("__files__", []):
        counter += 1
        cid = f"Cmp_{counter}"
        fid = f"Fil_{counter}"
        components.append(cid)
        xml += (
            f'<Component Id="{cid}" Guid="{guid(str(source))}">'
            f'<File Id="{fid}" Source={quoteattr(str(source))} Name={quoteattr(filename)} KeyPath="yes"/>'
            "</Component>"
        )
    return xml

install_tree = emit(tree)
component_refs = "".join(f'<ComponentRef Id="{c}"/>' for c in components)

doc = f'''<?xml version="1.0" encoding="utf-8"?>
<Wix xmlns="http://schemas.microsoft.com/wix/2006/wi">
  <Product Id="*" Name={quoteattr(product)} Language="2052" Version="{version}"
           Manufacturer={quoteattr(manufacturer)}
           UpgradeCode="{guid("upgrade")}">
    <Package Id="*" Keywords="Installer" Description={quoteattr(product)}
             Manufacturer={quoteattr(manufacturer)} InstallerVersion="500"
             Languages="2052" Compressed="yes" InstallScope="perMachine"/>
    <Media Id="1" Cabinet="starlit.cab" EmbedCab="yes"/>
    <Directory Id="TARGETDIR" Name="SourceDir">
      <Directory Id="ProgramFiles64Folder">
        <Directory Id="INSTALLDIR" Name={quoteattr(product)}>
{install_tree}
        </Directory>
      </Directory>
      <Directory Id="ProgramMenuFolder">
        <Directory Id="AppMenuDir" Name={quoteattr(product)}>
          <Component Id="CmpStartMenu" Guid="{guid("shortcut/startmenu")}">
            <Shortcut Id="StartMenuShortcut" Name={quoteattr(product)}
                      Target="[INSTALLDIR]starlit-foss.exe" WorkingDirectory="INSTALLDIR"/>
            <RemoveFolder Id="RemoveAppMenuDir" On="uninstall"/>
            <RegistryValue Root="HKCU" Key="Software\\StarlitFOSS" Name="startmenu"
                           Type="integer" Value="1" KeyPath="yes"/>
          </Component>
        </Directory>
      </Directory>
      <Directory Id="DesktopFolder">
        <Component Id="CmpDesktop" Guid="{guid("shortcut/desktop")}">
          <Shortcut Id="DesktopShortcut" Name={quoteattr(product)}
                    Target="[INSTALLDIR]starlit-foss.exe" WorkingDirectory="INSTALLDIR"/>
          <RegistryValue Root="HKCU" Key="Software\\StarlitFOSS" Name="desktop"
                         Type="integer" Value="1" KeyPath="yes"/>
        </Component>
      </Directory>
    </Directory>
    <Feature Id="MainFeature" Title={quoteattr(product)} Level="1">
      <ComponentRef Id="CmpStartMenu"/>
      <ComponentRef Id="CmpDesktop"/>
      {component_refs}
    </Feature>
  </Product>
</Wix>
'''

Path(wxs).write_text(doc, encoding="utf-8")
print(f"文件数: {len(files)}")
PY

echo "调用 wixl …"
wixl -a x64 -o "$OUT" "$WXS"
ls -lh "$OUT"
echo
echo "完成：$OUT"
echo "校验：msiinfo export '$OUT' Property"
echo "说明：卸载不会删除 %APPDATA%\\starlit-desktop，用户数据默认保留。"
