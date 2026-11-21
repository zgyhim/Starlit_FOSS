#!/bin/sh
# Windows MSI：用 wixl(msitools) 生成，无需 Windows 或 Wine。
set -eu

. "$(dirname "$0")/../lib/common.sh"

command -v wixl >/dev/null || { echo "missing wixl (pkg install msitools / apt install msitools)" >&2; exit 1; }
command -v python3 >/dev/null || { echo "missing python3" >&2; exit 1; }

WIN_DIR=${WIN_DIR:-$ROOT/src-tauri/target/x86_64-pc-windows-gnu/release}
EXE=${STARLIT_EXE:-$WIN_DIR/starlit-foss.exe}
require_file "$EXE"

PRODUCT_NAME=${PRODUCT_NAME:-Starlit FOSS}
MANUFACTURER=${MANUFACTURER:-zgyhim}
WXS=$WORK/starlit-msi.wxs
OUT=$DIST/StarlitFOSS-$VERSION-x64.msi

echo "exe:    $EXE"
echo "version: $VERSION"
echo "media:  $([ "$INCLUDE_MEDIA" = 1 ] && echo included || echo skipped)"

python3 - "$EXE" "$VERSION" "$PRODUCT_NAME" "$MANUFACTURER" "$INCLUDE_MEDIA" "$WXS" "$ROOT" <<'PY'
import sys, uuid
from pathlib import Path
from xml.sax.saxutils import escape, quoteattr

exe, version, product, manufacturer, include_media, wxs, root_arg = sys.argv[1:8]
root = Path(root_arg)
files = [(Path(exe).resolve(), "starlit-foss.exe")]

template = root / "resources" / "template"
for path in sorted(template.rglob("*")):
    if path.is_file():
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

# 目录名与 meta.json 不一致的非 ASCII 目录是编码损坏的重复副本，跳过。
import json
chars = template / "persona" / "characters"
bad = []
if chars.is_dir():
    for child in sorted(chars.iterdir()):
        if not child.is_dir():
            continue
        try:
            name = json.loads((child / "meta.json").read_text(encoding="utf-8")).get("name")
        except Exception:
            name = None
        if name and name != child.name and not child.name.isascii():
            bad.append(child)
if bad:
    for d in bad:
        print(f"skip mis-encoded directory: {d.name!r}", file=sys.stderr)
    files = [(s, r) for s, r in files if not any(d in s.parents for d in bad)]

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
refs = "".join(f'<ComponentRef Id="{c}"/>' for c in components)

doc = f'''<?xml version="1.0" encoding="utf-8"?>
<Wix xmlns="http://schemas.microsoft.com/wix/2006/wi">
  <Product Id="*" Name={quoteattr(product)} Language="2052" Version="{version}"
           Manufacturer={quoteattr(manufacturer)} UpgradeCode="{guid("upgrade")}">
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
            <RegistryValue Root="HKCU" Key="Software\\\\StarlitFOSS" Name="startmenu"
                           Type="integer" Value="1" KeyPath="yes"/>
          </Component>
        </Directory>
      </Directory>
      <Directory Id="DesktopFolder">
        <Component Id="CmpDesktop" Guid="{guid("shortcut/desktop")}">
          <Shortcut Id="DesktopShortcut" Name={quoteattr(product)}
                    Target="[INSTALLDIR]starlit-foss.exe" WorkingDirectory="INSTALLDIR"/>
          <RegistryValue Root="HKCU" Key="Software\\\\StarlitFOSS" Name="desktop"
                         Type="integer" Value="1" KeyPath="yes"/>
        </Component>
      </Directory>
    </Directory>
    <Feature Id="MainFeature" Title={quoteattr(product)} Level="1">
      <ComponentRef Id="CmpStartMenu"/>
      <ComponentRef Id="CmpDesktop"/>
      {refs}
    </Feature>
  </Product>
</Wix>
'''
Path(wxs).write_text(doc, encoding="utf-8")
print(f"files: {len(files)}")
PY

wixl -a x64 -o "$OUT" "$WXS"
echo "msi: $OUT"
ls -lh "$OUT"
