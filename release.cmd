SET VERSION=0.0.1

RMDIR /Q /S release || exit /b
MKDIR release\virterm-%VERSION%-win64 || exit /b

:: Windows 64

cargo build --release || exit /b

COPY target\release\virterm.exe release\virterm-%VERSION%-win64\virterm.exe || exit /b

:: upx --brute release\virterm-%VERSION%-win64\virterm.exe || exit /b

tar.exe -a -c -f release\virterm-%VERSION%-win64.zip -C release\virterm-%VERSION%-win64 virterm.exe
