; Windows installer hook: VC++ runtime + optional bundled Vulkan Runtime installer.
; VoiceTypr's main executable is CPU-safe; Vulkan is isolated in a sidecar.

!include "LogicLib.nsh"

!macro InstallVcRedist
    ClearErrors
    SetRegView 64
    ReadRegDWord $0 HKLM "SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64" "Installed"
    ${If} ${Errors}
        StrCpy $0 0
    ${EndIf}

    ${If} $0 == 1
        DetailPrint "Visual C++ Runtime already installed"
        Goto vcredist_done
    ${EndIf}

    ${If} ${FileExists} "$INSTDIR\resources\windows\resources\vc_redist.x64.exe"
        DetailPrint "Installing Microsoft Visual C++ Runtime..."
        CopyFiles /SILENT "$INSTDIR\resources\windows\resources\vc_redist.x64.exe" "$TEMP\vc_redist.x64.exe"
        ExecWait '"$TEMP\vc_redist.x64.exe" /install /passive /norestart' $1

        ${If} $1 == 0
            DetailPrint "Visual C++ Runtime installed successfully"
        ${ElseIf} $1 == 3010
            DetailPrint "Visual C++ Runtime installed (restart required)"
        ${ElseIf} $1 == 1638
            DetailPrint "Visual C++ Runtime already installed (newer or same version)"
        ${Else}
            DetailPrint "Visual C++ Runtime installer returned code $1"
        ${EndIf}

        Delete "$TEMP\vc_redist.x64.exe"
    ${Else}
        DetailPrint "vc_redist.x64.exe not bundled, skipping runtime installation"
    ${EndIf}

    vcredist_done:
!macroend

!macro InstallVulkanRuntime
    ${If} ${FileExists} "$INSTDIR\resources\windows\resources\VulkanRT-Installer.exe"
        DetailPrint "Installing Vulkan Runtime for optional GPU acceleration..."
        CopyFiles /SILENT "$INSTDIR\resources\windows\resources\VulkanRT-Installer.exe" "$TEMP\VulkanRT-Installer.exe"
        ExecWait '"$TEMP\VulkanRT-Installer.exe" /S' $1

        ${If} $1 == 0
            DetailPrint "Vulkan Runtime installed successfully"
        ${ElseIf} $1 == 3010
            DetailPrint "Vulkan Runtime installed (restart required)"
        ${ElseIf} $1 == 1638
            DetailPrint "Vulkan Runtime already installed (newer or same version)"
        ${Else}
            DetailPrint "Vulkan Runtime installer returned code $1; VoiceTypr will use CPU fallback if GPU acceleration is unavailable"
        ${EndIf}

        Delete "$TEMP\VulkanRT-Installer.exe"
    ${Else}
        DetailPrint "VulkanRT-Installer.exe not bundled, skipping Vulkan Runtime installation"
    ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_POSTINSTALL
    !insertmacro InstallVcRedist
    !insertmacro InstallVulkanRuntime
!macroend

!macro NSIS_HOOK_PREUNINSTALL
!macroend
