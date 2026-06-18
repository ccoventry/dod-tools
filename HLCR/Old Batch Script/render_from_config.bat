@echo off
setlocal enabledelayedexpansion

echo.
echo ============================================================
echo         Half-Life Clip Auto Render Tool (Config Mode)
echo ============================================================
echo.

:: Default config file
set "configfile=render_config.ini"

:: If user passed an argument, use that (e.g. render_from_config.bat render_config_exhibition.ini)
if not "%~1"=="" set "configfile=%~1"

if not exist "%configfile%" (
    echo ERROR: Config file "%configfile%" not found.
    pause
    exit /b
)

echo Loading configuration from "%configfile%" ...
for /f "usebackq tokens=1* delims==" %%A in ("%configfile%") do (
    set "key=%%A"
    set "val=%%B"
    if defined key if not "!key:~0,1!"=="#" (
        for /f "tokens=* delims= " %%x in ("!key!") do set "key=%%x"
        for /f "tokens=* delims= " %%y in ("!val!") do set "val=%%y"
        set "!key!=!val!"
    )
)

:: Validate key variables
if not exist "%ffmpeg_path%" (
    echo.
    echo ERROR: ffmpeg not found at "%ffmpeg_path%"
    pause
    exit /b
)

if not exist "%source_folder%" (
    echo.
    echo ERROR: Source folder not found "%source_folder%"
    pause
    exit /b
)

if not exist "%output_folder%" (
    echo.
    echo Output folder not found. Creating it...
    mkdir "%output_folder%"
)

echo.
echo ============================================
echo  FFmpeg Path: %ffmpeg_path%
echo  Source:      %source_folder%
echo  Destination: %output_folder%
echo  Frame Rate:  %fps% FPS
echo ============================================
echo.

:: Prevent computer from sleeping while running
set "lockfile=%TEMP%\%~n0_keepawake_%RANDOM%.lock"
type nul > "%lockfile%"
echo Lock file active: "%lockfile%"
start "" /b powershell -NoProfile -Command "$w=Add-Type -MemberDefinition '[DllImport(\"kernel32.dll\")] public static extern uint SetThreadExecutionState(uint esFlags);' -Name 'Win32' -Namespace Win32 -PassThru; $w::SetThreadExecutionState(0x80000003); while (Test-Path '%lockfile%') { Start-Sleep -Seconds 5 }; $w::SetThreadExecutionState(0x80000000)" >nul 2>&1

echo Counting files to process...
set "total_files=0"
for /r "%source_folder%" %%A in (*.wav) do (
    set "takefolder=%%~dpA"

    REM Sanitize path for use in a variable name
    set "tf_id=!takefolder:\=_!"
    set "tf_id=!tf_id::=_!"

    if not defined counted_!tf_id! (
        set "has_images=0"
        for /d %%F in ("!takefolder!\*") do (
            if exist "%%F\00000.bmp" (
                set /a total_files+=1
                set "has_images=1"
            )
        )
        if !has_images! equ 1 (
            set counted_!tf_id!=1
        )
    )
)
echo Found !total_files! clips to render.
echo.

set "current_file=0"
for /r "%source_folder%" %%A in (*.wav) do (
    set "takefolder=%%~dpA"

    REM Sanitize path for use in a variable name
    set "tf_id=!takefolder:\=_!"
    set "tf_id=!tf_id::=_!"

    if not defined processed_!tf_id! (
        set "has_images_to_process=0"
        for /d %%F in ("!takefolder!\*") do (
            if exist "%%F\00000.bmp" (
                set "has_images_to_process=1"
            )
        )

        if !has_images_to_process! equ 1 (
            set processed_!tf_id!=1
            
            REM Prioritize sound.wav, otherwise use the first .wav found (%%A)
            set "wav_to_use=%%~fA"
            set "wavname_to_use=%%~nA"
            if exist "!takefolder!sound.wav" (
                set "wav_to_use=!takefolder!sound.wav"
                set "wavname_to_use=sound"
            )

            for /d %%F in ("!takefolder!\*") do (
                set "imageFolder=%%F"
                set "imageFolderName=%%~nF"
                if exist "!imageFolder!\00000.bmp" (
                    set /a current_file+=1
                    
                    for %%B in ("!takefolder!\..") do set "demoFolder=%%~fB"
                    for %%C in ("!demoFolder!") do set "demoName=%%~nC"
                    for %%D in ("!takefolder!.") do set "takeName=%%~nxD"

                    for /f %%N in ('dir /b /a-d "!imageFolder!\*.bmp" ^| find /c /v ""') do set "frameCount=%%N"

                    if /i "!wavname_to_use!"=="sound" (
                        set "baseName=!demoName!-!takeName!-!wavname_to_use!"
                    ) else {
                        set "baseName=!wavname_to_use!"
                    )
                    set "finalName=!baseName!-!imageFolderName!.mov"

                    if exist "%output_folder%\!finalName!" (
                        call :FindUniqueName "%output_folder%" "!baseName!-!imageFolderName!" ".mov"
                    )

                    pushd "!takefolder!"
                    
                    set "FFMPEG_PATH=%ffmpeg_path%"
                    set "OUT_FILE=%output_folder%\!finalName!"
                    set "IN_WAV=!wavname_to_use!.wav"
                    set "FPS=!fps!"
                    set "PREFIX=[!current_file!/!total_files!] Rendering !finalName!"
                    
                    powershell -NoProfile -Command ^
                        "$total=[int]$env:frameCount; $p=$env:PREFIX; Write-Host -NoNewline ('{0}: 0%%' -f $p); " ^
                        "$fArgs=@('-y','-framerate',$env:FPS,'-i','!imageFolderName!\%%05d.bmp','-i',$env:IN_WAV,'-c:v','prores_ks','-profile:v','3','-pix_fmt','yuv422p10le','-c:a','pcm_s16le','-shortest','-movflags','+faststart',$env:OUT_FILE,'-progress','pipe:1','-loglevel','error'); " ^
                        "& $env:FFMPEG_PATH $fArgs | ForEach-Object { if ($_ -match '^frame=(\d+)') { $pct=[math]::Min(100, [math]::Round([int]$matches[1]*100/$total)); Write-Host -NoNewline ('{0}{1}: {2}%%' -f [char]13, $p, $pct) } }; Write-Host ''"
                        
                    popd
                )
            )
        )
    )
)


echo.
echo All done!
if exist "%lockfile%" del "%lockfile%"
pause
exit /b

:FindUniqueName
set "cnt=1"
:loopUnique
set "finalName=%~2_%cnt%%~3"
if exist "%~1\!finalName!" (
    set /a cnt+=1
    goto loopUnique
)
exit /b
