@echo off

(
    start "login" cmd /C "kodama-login.exe"
    start "patch" cmd /C "kodama-patch.exe"
    start "web" cmd /C "kodama-web.exe"
    start "lobby" cmd /C "kodama-lobby.exe"
    start "world" cmd /C "kodama-world.exe"
) | pause
