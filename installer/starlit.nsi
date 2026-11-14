; 星伴 FOSS — 独立 NSIS 安装脚本
; 用法（在仓库根目录）：
;   makensis /DAPP_SOURCE=src-tauri\target\release installer\starlit.nsi
; 可选：把 MicrosoftEdgeWebview2Setup.exe 放到生成目录，安装时会检测并静默安装。

Unicode true
!include "MUI2.nsh"
!include "x64.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"

!define APP_NAME        "星伴 FOSS"
!define APP_EXE         "starlit-foss.exe"
!define APP_VERSION     "0.1.0"
!define APP_PUBLISHER   "Starlit FOSS"
!define APP_ID          "StarlitFOSS"
!define APP_DATA_DIR    "starlit-desktop"
!define WEBVIEW2_GUID   "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"

!ifndef APP_SOURCE
  !define APP_SOURCE "..\src-tauri\target\release"
!endif
!ifndef OUTPUT_DIR
  !define OUTPUT_DIR "."
!endif

Name "${APP_NAME}"
BrandingText "${APP_NAME} ${APP_VERSION}"
OutFile "${OUTPUT_DIR}\StarlitFOSS-Setup-${APP_VERSION}.exe"
InstallDir "$PROGRAMFILES64\${APP_NAME}"
InstallDirRegKey HKLM "Software\${APP_ID}" "InstallDir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma
SetCompressorDictSize 64

VIProductVersion "0.1.0.0"
VIAddVersionKey "ProductName"     "${APP_NAME}"
VIAddVersionKey "ProductVersion"  "${APP_VERSION}"
VIAddVersionKey "CompanyName"     "${APP_PUBLISHER}"
VIAddVersionKey "FileDescription" "${APP_NAME} 安装程序"
VIAddVersionKey "FileVersion"     "${APP_VERSION}"
VIAddVersionKey "LegalCopyright"  "AGPL-3.0"

!define MUI_ICON "..\src-tauri\icons\icon.ico"
!define MUI_UNICON "..\src-tauri\icons\icon.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\${APP_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "启动 ${APP_NAME}"
!define MUI_LICENSEPAGE_TEXT_TOP "本项目以 GNU AGPL v3.0 发布。"
!define MUI_LICENSEPAGE_BUTTON "同意并继续"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "..\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "星伴 FOSS 仅支持 64 位 Windows。"
    Abort
  ${EndIf}
  SetRegView 64
  ; 已安装旧版本时沿用原目录
  ReadRegStr $0 HKLM "Software\${APP_ID}" "InstallDir"
  ${If} $0 != ""
    StrCpy $INSTDIR $0
  ${EndIf}
FunctionEnd

Function CheckWebView2
  ReadRegStr $0 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\${WEBVIEW2_GUID}" "pv"
  ${If} $0 == ""
    ReadRegStr $0 HKCU "Software\Microsoft\EdgeUpdate\Clients\${WEBVIEW2_GUID}" "pv"
  ${EndIf}
  ${If} $0 != ""
    DetailPrint "已检测到 WebView2 Runtime $0"
    Return
  ${EndIf}
  ${If} ${FileExists} "$EXEDIR\MicrosoftEdgeWebview2Setup.exe"
    DetailPrint "正在安装 WebView2 Runtime…"
    ExecWait '"$EXEDIR\MicrosoftEdgeWebview2Setup.exe" /silent /install'
  ${Else}
    MessageBox MB_ICONEXCLAMATION|MB_OK "未检测到 Microsoft Edge WebView2 Runtime。$\r$\n$\r$\n请先从微软官方页面安装 Evergreen Runtime 后再运行星伴：$\r$\nhttps://developer.microsoft.com/microsoft-edge/webview2/"
  ${EndIf}
FunctionEnd

Section "星伴 FOSS" SecMain
  SectionIn RO
  Call CheckWebView2

  SetOutPath "$INSTDIR"
  File /r "${APP_SOURCE}\*.*"

  CreateDirectory "$SMPROGRAMS\${APP_NAME}"
  CreateShortCut "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}"
  CreateShortCut "$SMPROGRAMS\${APP_NAME}\卸载 ${APP_NAME}.lnk" "$INSTDIR\Uninstall.exe"
  CreateShortCut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}"

  WriteUninstaller "$INSTDIR\Uninstall.exe"

  WriteRegStr HKLM "Software\${APP_ID}" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "Software\${APP_ID}" "Version" "${APP_VERSION}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}" "DisplayName" "${APP_NAME}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}" "Publisher" "${APP_PUBLISHER}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}" "DisplayIcon" "$INSTDIR\${APP_EXE}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}" "NoModify" 1
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  SetRegView 64
  Delete "$DESKTOP\${APP_NAME}.lnk"
  RMDir /r "$SMPROGRAMS\${APP_NAME}"

  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}"
  DeleteRegKey HKLM "Software\${APP_ID}"

  ; 用户数据默认保留，避免卸载时误删聊天与记忆
  MessageBox MB_ICONQUESTION|MB_YESNO "是否同时删除本机的聊天记录、记忆和设置？$\r$\n$\r$\n位置：$APPDATA\${APP_DATA_DIR}" IDNO KeepData
  RMDir /r "$APPDATA\${APP_DATA_DIR}"
KeepData:
  RMDir /r "$INSTDIR"
SectionEnd
