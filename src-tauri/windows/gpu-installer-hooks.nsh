; GPU Installer - MUST install Vulkan first

!include "LogicLib.nsh"

Var VulkanInstalled
Var GPUCompatible

; Check for Vulkan before anything else
!macro NSIS_HOOK_PREINSTALL
    ; Initialize variables
    StrCpy $VulkanInstalled "NO"
    StrCpy $GPUCompatible "UNKNOWN"
    
    ; Phase 1: Check if Vulkan Runtime is properly installed
    ; Check registry for Vulkan Runtime
    ReadRegStr $0 HKLM "SOFTWARE\Khronos\Vulkan\Drivers" ""
    ${If} $0 != ""
        ; Also verify the DLL exists
        ${If} ${FileExists} "$SYSDIR\vulkan-1.dll"
            StrCpy $VulkanInstalled "YES"
        ${EndIf}
    ${EndIf}
    
    ; Phase 2: Check GPU compatibility (using WMI)
    ; Check for NVIDIA, AMD, or Intel Arc GPUs that support Vulkan
    nsExec::ExecToStack 'wmic path win32_VideoController get name /value | findstr /i "NVIDIA AMD Radeon Intel(R) Arc"'
    Pop $0 ; Exit code
    Pop $1 ; Output
    
    ${If} $0 == 0
        StrCpy $GPUCompatible "YES"
        DetailPrint "Compatible GPU detected: $1"
    ${Else}
        StrCpy $GPUCompatible "NO"
        DetailPrint "No compatible GPU detected for Vulkan acceleration"
    ${EndIf}
    
    ${If} $VulkanInstalled == "NO"
        ; Check if GPU is compatible before proceeding
        ${If} $GPUCompatible == "NO"
            MessageBox MB_OKCANCEL|MB_ICONEXCLAMATION "Warning: No Compatible GPU Detected!$\n$\n\
Your system doesn't appear to have a Vulkan-compatible GPU.$\n\
GPU acceleration may not work even after installing Vulkan.$\n$\n\
Detected GPUs that support Vulkan:$\n\
• NVIDIA GPUs (GTX 600 series or newer)$\n\
• AMD GPUs (Radeon HD 7000 series or newer)$\n\
• Intel Arc GPUs$\n$\n\
Click OK to continue anyway or Cancel to abort." IDCANCEL abort_install
        ${EndIf}
        
        MessageBox MB_OK|MB_ICONEXCLAMATION "Vulkan Runtime Required!$\n$\n\
VoiceTypr GPU version requires Vulkan Runtime to work.$\n$\n\
The installer will now download and install Vulkan Runtime.$\n\
This is required for 5-10x faster transcription.$\n$\n\
Click OK to continue."
        
        download_vulkan:
        ; Download Vulkan ZIP with retry logic
        DetailPrint "Downloading Vulkan Runtime (required for GPU acceleration)..."
        DetailPrint "This may take a minute depending on your connection..."
        
        ; Download the ZIP file containing Vulkan installer
        NSISdl::download /TIMEOUT=120000 \
            "https://lwzw81mcpu.ufs.sh/f/KhPlod6OD3vGP1UvO1HK0uemdxzVr9QI61tsAFw34n8pkXHv" \
            "$TEMP\vulkan-runtime.zip"
        
        Pop $0
        ${If} $0 != "success"
            ; For now, same URL as fallback (you can change this to a mirror)
            DetailPrint "Retrying download..."
            NSISdl::download /TIMEOUT=120000 \
                "https://lwzw81mcpu.ufs.sh/f/KhPlod6OD3vGP1UvO1HK0uemdxzVr9QI61tsAFw34n8pkXHv" \
                "$TEMP\vulkan-runtime.zip"
            
            Pop $0
            ${If} $0 != "success"
                MessageBox MB_RETRYCANCEL|MB_ICONSTOP "Failed to download Vulkan Runtime!$\n$\n\
Network issue or server unavailable.$\n$\n\
Click Retry to try again, or Cancel to abort.$\n\
You can also install Vulkan manually from:$\n\
https://vulkan.lunarg.com/sdk/home" IDRETRY retry_download
                Abort
                
                retry_download:
                Goto download_vulkan
            ${EndIf}
        ${EndIf}
        
        ; Verify file size (should be ~30MB)
        FileOpen $1 "$TEMP\vulkan-runtime.zip" r
        FileSeek $1 0 END $2
        FileClose $1
        ${If} $2 < 10000000  ; Less than 10MB indicates corrupt download
            Delete "$TEMP\vulkan-runtime.zip"
            MessageBox MB_OK|MB_ICONSTOP "Downloaded file appears corrupt!$\n$\n\
Please check your internet connection and try again."
            Abort
        ${EndIf}
        
        DetailPrint "Download completed successfully"
        
        ; Extract the installer from ZIP
        DetailPrint "Extracting Vulkan installer..."
        
        ; Create extraction directory
        CreateDirectory "$TEMP\vulkan-extract"
        
        ; Use nsisunz plugin to extract ZIP (if available in Tauri)
        ; If nsisunz is not available, fall back to PowerShell
        ClearErrors
        nsisunz::Unzip "$TEMP\vulkan-runtime.zip" "$TEMP\vulkan-extract"
        Pop $1
        
        ${If} $1 != "success"
            ; Fallback: Use PowerShell to extract
            DetailPrint "Using PowerShell to extract..."
            nsExec::ExecToStack 'powershell -Command "Expand-Archive -Path \"$TEMP\vulkan-runtime.zip\" -DestinationPath \"$TEMP\vulkan-extract\" -Force"'
            Pop $1
            Pop $2
            
            ${If} $1 != 0
                MessageBox MB_OK|MB_ICONSTOP "Failed to extract Vulkan installer!$\n$\n\
Please check if the download was successful."
                Delete "$TEMP\vulkan-runtime.zip"
                Abort
            ${EndIf}
        ${EndIf}
        
        ; Find the .exe installer in extracted files
        FindFirst $3 $4 "$TEMP\vulkan-extract\*.exe"
        ${If} $4 == ""
            MessageBox MB_OK|MB_ICONSTOP "No installer found in ZIP file!"
            Abort
        ${EndIf}
        FindClose $3
        
        ; Install Vulkan - this will show in the installer progress
        DetailPrint "Installing Vulkan Runtime..."
        DetailPrint "Please wait, this may take a minute..."
        
        ; Run Vulkan installer silently
        ExecWait '"$TEMP\vulkan-extract\$4" /S' $0
        
        ; Show progress
        DetailPrint "Vulkan Runtime installation completed with exit code: $0"
        DetailPrint "Verifying installation..."
        
        ${If} $0 != 0
            ; Different messages for different error codes
            ${If} $0 == 1223  ; User cancelled
                MessageBox MB_OK|MB_ICONEXCLAMATION "Vulkan installation cancelled!$\n$\n\
The Vulkan installer requires administrator privileges.$\n\
Please run this installer as Administrator."
                Abort
            ${ElseIf} $0 == 3010  ; Reboot required
                MessageBox MB_OK|MB_ICONINFORMATION "Vulkan installed - Reboot required!$\n$\n\
Vulkan Runtime was installed but requires a system restart.$\n\
Please reboot and run this installer again."
                Abort
            ${Else}
                MessageBox MB_RETRYCANCEL|MB_ICONSTOP "Vulkan installation failed! (Error: $0)$\n$\n\
This might be due to:$\n\
• Insufficient permissions (try Run as Administrator)$\n\
• Antivirus blocking the installation$\n\
• Corrupted download$\n$\n\
Retry to download and try again, or Cancel to abort." IDRETRY retry_vulkan_install
                Abort
            ${EndIf}
        ${EndIf}
        
        ; Clean up temporary files
        Delete "$TEMP\vulkan-runtime.zip"
        RMDir /r "$TEMP\vulkan-extract"
        
        Goto vulkan_check_done
        
        abort_install:
        MessageBox MB_OK|MB_ICONSTOP "Installation Cancelled$\n$\n\
The GPU version requires a compatible GPU.$\n\
Please install the CPU version instead."
        Abort
        
        vulkan_check_done:
        
        ; Verify Vulkan is actually installed
        ${If} ${FileExists} "$SYSDIR\vulkan-1.dll"
            ; Double-check registry for proper installation
            ReadRegStr $1 HKLM "SOFTWARE\Khronos\Vulkan\Drivers" ""
            ${If} $1 != ""
                DetailPrint "✓ Vulkan Runtime verified successfully!"
                DetailPrint "✓ Registry entries confirmed"
                DetailPrint "✓ GPU acceleration ready!"
                MessageBox MB_OK|MB_ICONINFORMATION "✓ Vulkan Runtime installed successfully!$\n$\n\
VoiceTypr GPU version will now install with 5-10x faster transcription!"
            ${Else}
                DetailPrint "Warning: Vulkan DLL found but registry entries missing"
                MessageBox MB_OK|MB_ICONEXCLAMATION "Vulkan installation incomplete!$\n$\n\
The files were copied but registration may have failed.$\n\
GPU acceleration might not work properly.$\n$\n\
You may need to reinstall Vulkan manually."
            ${EndIf}
        ${Else}
            DetailPrint "ERROR: Vulkan DLL not found after installation!"
            MessageBox MB_ABORTRETRYIGNORE|MB_ICONSTOP "Vulkan installation failed!$\n$\n\
The Vulkan Runtime did not install correctly.$\n$\n\
Abort = Cancel installation$\n\
Retry = Try installing Vulkan again$\n\
Ignore = Continue anyway (GPU won't work)" IDRETRY retry_vulkan_install IDIGNORE ignore_error
            Abort
            
            retry_vulkan_install:
            Delete "$TEMP\vulkan-runtime.zip"
            RMDir /r "$TEMP\vulkan-extract"
            Goto download_vulkan
            
            ignore_error:
            DetailPrint "Continuing without GPU acceleration..."
        ${EndIf}
    ${Else}
        ; Vulkan already installed - just show progress, no modal
        DetailPrint "Checking system requirements..."
        DetailPrint "✓ Vulkan Runtime detected"
        
        ; Show GPU compatibility status
        ${If} $GPUCompatible == "YES"
            DetailPrint "✓ Compatible GPU detected"
            DetailPrint "✓ GPU acceleration available"
            DetailPrint "Ready for 5-10x faster transcription!"
        ${Else}
            DetailPrint "⚠ Warning: No compatible GPU detected"
            DetailPrint "GPU acceleration may not work properly"
        ${EndIf}
    ${EndIf}
!macroend