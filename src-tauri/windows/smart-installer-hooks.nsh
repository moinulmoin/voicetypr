; Smart Installer - Detects GPU and informs about acceleration options
; No automatic installations - respects user's system

!include "LogicLib.nsh"

Var GPUDetected
Var VulkanInstalled
Var GPUName

; Smart detection and user information
!macro NSIS_HOOK_PREINSTALL
    ; Initialize variables
    StrCpy $GPUDetected "NO"
    StrCpy $VulkanInstalled "NO"
    StrCpy $GPUName "Unknown"
    
    ; Phase 1: Check for GPU hardware
    DetailPrint "Detecting system capabilities..."
    
    ; Check for dedicated GPU (with significant memory)
    ; This catches all dedicated GPUs, not just specific brands
    nsExec::ExecToStack 'wmic path win32_VideoController where "AdapterRAM > 1000000000" get name /value'
    Pop $0 ; Exit code
    Pop $1 ; Output
    
    ${If} $0 == 0
        ${AndIf} $1 != ""
        StrCpy $GPUDetected "YES"
        ; Simple GPU detection message
        StrCpy $GPUName "GPU"
        DetailPrint "Dedicated GPU detected"
    ${Else}
        DetailPrint "No dedicated GPU detected - will use CPU mode"
        Goto install_complete
    ${EndIf}
    
    ; Phase 2: Check if Vulkan Runtime is installed
    ${If} ${FileExists} "$SYSDIR\vulkan-1.dll"
        StrCpy $VulkanInstalled "YES"
        DetailPrint "✓ Vulkan Runtime detected - GPU acceleration ready!"
        
        ; Everything is ready - no need to show a message, just continue
        ; The app will automatically use GPU acceleration
        Goto install_complete
    ${Else}
        ; GPU exists but Vulkan missing - inform user
        DetailPrint "GPU detected but Vulkan Runtime not found"
        
        MessageBox MB_OK|MB_ICONINFORMATION "GPU Acceleration Available$\n$\n\
Good news! Your $GPUName can make VoiceTypr 5-10x faster.$\n$\n\
To enable GPU acceleration, please update your graphics drivers:$\n$\n\
• NVIDIA: nvidia.com/drivers$\n\
• AMD: amd.com/support$\n\
• Intel: intel.com/content/www/us/en/support$\n$\n\
Modern graphics drivers include Vulkan Runtime automatically.$\n\
After updating, VoiceTypr will use your GPU!"
    ${EndIf}
    
    install_complete:
    ; Continue with normal installation
!macroend

; No special uninstall handling needed
!macro NSIS_HOOK_PREUNINSTALL
!macroend

