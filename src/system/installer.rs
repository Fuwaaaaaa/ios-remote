use tracing::info;

/// Windows installer helpers.
///
/// Generates NSIS script for creating a Windows installer.
/// Run: `makensis installer.nsi` to create Setup.exe.
pub fn generate_nsis_script() -> String {
    let version = env!("CARGO_PKG_VERSION");
    format!(r#"
!include "MUI2.nsh"

Name "ios-remote {version}"
OutFile "ios-remote-{version}-setup.exe"
InstallDir "$PROGRAMFILES64\ios-remote"
RequestExecutionLevel admin

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "Japanese"

Section "Install"
    SetOutPath "$INSTDIR"
    File "target\release\ios-remote.exe"
    File "README.md"

    ; Create Start Menu shortcut
    CreateDirectory "$SMPROGRAMS\ios-remote"
    CreateShortCut "$SMPROGRAMS\ios-remote\ios-remote.lnk" "$INSTDIR\ios-remote.exe"
    CreateShortCut "$SMPROGRAMS\ios-remote\Uninstall.lnk" "$INSTDIR\uninstall.exe"

    ; Create Desktop shortcut
    CreateShortCut "$DESKTOP\ios-remote.lnk" "$INSTDIR\ios-remote.exe"

    ; Firewall rules
    nsExec::Exec 'netsh advfirewall firewall add rule name="ios-remote" dir=in action=allow program="$INSTDIR\ios-remote.exe"'

    ; Write uninstaller
    WriteUninstaller "$INSTDIR\uninstall.exe"

    ; Registry for Add/Remove Programs
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ios-remote" "DisplayName" "ios-remote"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ios-remote" "UninstallString" "$INSTDIR\uninstall.exe"
    WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ios-remote" "DisplayVersion" "{version}"
SectionEnd

Section "Uninstall"
    Delete "$INSTDIR\ios-remote.exe"
    Delete "$INSTDIR\README.md"
    Delete "$INSTDIR\uninstall.exe"
    RMDir "$INSTDIR"
    Delete "$SMPROGRAMS\ios-remote\*.*"
    RMDir "$SMPROGRAMS\ios-remote"
    Delete "$DESKTOP\ios-remote.lnk"
    nsExec::Exec 'netsh advfirewall firewall delete rule name="ios-remote"'
    DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ios-remote"
SectionEnd
"#)
}

/// Write the NSIS script to disk.
pub fn write_installer_script() -> Result<String, String> {
    let script = generate_nsis_script();
    let path = "installer.nsi";
    std::fs::write(path, script).map_err(|e| e.to_string())?;
    info!(path = %path, "NSIS installer script generated. Run: makensis installer.nsi");
    Ok(path.to_string())
}
