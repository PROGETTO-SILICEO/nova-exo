@echo off
title Aggiornamento Chiavetta Nova-Exo
color 0B
echo ==========================================================
echo   AGGIORNAMENTO KERNEL NOVA-EXO SU CHIAVETTA USB (UEFI)
echo ==========================================================
echo.

set "VOL=\\?\Volume{0dcd0c86-948a-4627-bdf5-0c44ab7b6dd5}\"
set "SRC=E:\Users\alfor\Documents\GitHub\nova-exo\target\x86_64-unknown-none\release\nova-exo"
set "CONF=E:\Users\alfor\Documents\GitHub\nova-exo\limine.conf"

echo [1/4] Montaggio partizione EFI del volume USB su S:...
mountvol S: %VOL%

if not exist S:\ (
    echo.
    echo ATTENZIONE: Fai clic destro su questo file e seleziona "Esegui come amministratore".
    echo.
    pause
    exit /b 1
)

echo [2/4] Creazione cartelle boot...
if not exist S:\boot mkdir S:\boot
if not exist S:\boot\limine mkdir S:\boot\limine
if not exist S:\EFI\BOOT mkdir S:\EFI\BOOT

echo [3/4] Copia del kernel aggiornato e configurazioni...
copy /Y "%SRC%" S:\boot\nova-exo
copy /Y "%CONF%" S:\EFI\BOOT\limine.conf
copy /Y "%CONF%" S:\boot\limine\limine.conf

echo.
echo Controllo file copiato su S:\boot\nova-exo:
dir S:\boot\nova-exo | findstr /i "nova-exo"

echo.
echo [4/4] Smontaggio partizione EFI...
mountvol S: /D

echo.
echo ==========================================================
echo   CHIAVETTA USB AGGIORNATA CON SUCCESSO!
echo ==========================================================
echo.
pause
