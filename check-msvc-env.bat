@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 >nul
where cl.exe
where link.exe
"%USERPROFILE%\.cargo\bin\cargo.exe" --version
"%USERPROFILE%\.cargo\bin\rustc.exe" --version
