# deploy_to_usb.ps1
# Script di aggiornamento del kernel nova-exo sulla chiavetta USB (Disk 3)

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host "  AGGIORNAMENTO KERNEL NOVA-EXO SU CHIAVETTA USB (UEFI)" -ForegroundColor Yellow
Write-Host "==========================================================" -ForegroundColor Cyan

$VolumeGuid = "\\?\Volume{0dcd0c86-948a-4627-bdf5-0c44ab7b6dd5}\"
$DriveLetter = "S:"
$KernelSrc = "E:\Users\alfor\Documents\GitHub\nova-exo\target\x86_64-unknown-none\release\nova-exo"
$LimineConfSrc = "E:\Users\alfor\Documents\GitHub\nova-exo\limine.conf"

if (-not (Test-Path $KernelSrc)) {
    Write-Host "ERRORE: Binario kernel non trovato in $KernelSrc" -ForegroundColor Red
    Pause
    exit 1
}

Write-Host "[1/4] Montaggio partizione EFI del volume USB su ${DriveLetter}..." -ForegroundColor White
cmd /c "mountvol $DriveLetter $VolumeGuid"

if (-not (Test-Path "${DriveLetter}\")) {
    Write-Host "Tentativo con Set-Partition..." -ForegroundColor Yellow
    Set-Partition -DiskNumber 3 -PartitionNumber 1 -NewDriveLetter 'S' -ErrorAction SilentlyContinue
}

if (Test-Path "${DriveLetter}\") {
    Write-Host "[2/4] Creazione cartelle di boot su ${DriveLetter}..." -ForegroundColor Green
    New-Item -ItemType Directory -Path "${DriveLetter}\boot" -Force | Out-Null
    New-Item -ItemType Directory -Path "${DriveLetter}\boot\limine" -Force | Out-Null
    New-Item -ItemType Directory -Path "${DriveLetter}\EFI\BOOT" -Force | Out-Null

    Write-Host "[3/4] Copia del kernel aggiornato e configurazioni..." -ForegroundColor Green
    Copy-Item -Path $KernelSrc -Destination "${DriveLetter}\boot\nova-exo" -Force
    Copy-Item -Path $LimineConfSrc -Destination "${DriveLetter}\EFI\BOOT\limine.conf" -Force
    Copy-Item -Path $LimineConfSrc -Destination "${DriveLetter}\boot\limine\limine.conf" -Force

    $Copied = Get-Item "${DriveLetter}\boot\nova-exo"
    Write-Host "  -> File copiato con successo: $($Copied.Length) bytes ($($Copied.LastWriteTime))" -ForegroundColor Cyan

    Write-Host "[4/4] Smontaggio sicuro della partizione EFI..." -ForegroundColor White
    cmd /c "mountvol $DriveLetter /D"

    Write-Host "==========================================================" -ForegroundColor Green
    Write-Host "  CHIAVETTA USB AGGIORNATA CON SUCCESSO! PRONTA PER IL BOOT" -ForegroundColor Green
    Write-Host "==========================================================" -ForegroundColor Green
} else {
    Write-Host "ERRORE: Impossibile montare la partizione EFI sulla chiavetta." -ForegroundColor Red
}

Start-Sleep -Seconds 3
