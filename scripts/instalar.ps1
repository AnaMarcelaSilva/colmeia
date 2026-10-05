# Instala a Colmeia no Windows só para o seu usuário, sem administrador:
#   %LOCALAPPDATA%\Programs\Colmeia\colmeia.exe e colmeia-nucleo.exe
#   e o atalho "Colmeia" no menu Iniciar.
# Rode de dentro da pasta da release (com os .exe ao lado):
#   powershell -ExecutionPolicy Bypass -File .\instalar.ps1
$ErrorActionPreference = 'Stop'
$aqui = Split-Path -Parent $MyInvocation.MyCommand.Path
$destino = Join-Path $env:LOCALAPPDATA 'Programs\Colmeia'

foreach ($exe in 'colmeia.exe', 'colmeia-nucleo.exe') {
    if (-not (Test-Path (Join-Path $aqui $exe))) {
        Write-Error "não achei $exe ao lado deste script"
    }
}

# Um núcleo antigo rodando seguraria o arquivo e continuaria na versão
# anterior: ele é encerrado antes (os agentes abertos param junto).
$antigo = Join-Path $destino 'colmeia-nucleo.exe'
if (Test-Path $antigo) {
    & $antigo --encerrar 2>$null
    Start-Sleep -Seconds 1
}

New-Item -ItemType Directory -Force -Path $destino | Out-Null
Copy-Item (Join-Path $aqui 'colmeia.exe') $destino -Force
Copy-Item (Join-Path $aqui 'colmeia-nucleo.exe') $destino -Force

$menu = Join-Path ([Environment]::GetFolderPath('Programs')) 'Colmeia.lnk'
$shell = New-Object -ComObject WScript.Shell
$atalho = $shell.CreateShortcut($menu)
$atalho.TargetPath = Join-Path $destino 'colmeia.exe'
$atalho.WorkingDirectory = $destino
$atalho.Description = 'Colmeia: agentes de IA de código num lugar só'
$atalho.Save()

Write-Host "Colmeia instalada em $destino. Abra pelo menu Iniciar."
