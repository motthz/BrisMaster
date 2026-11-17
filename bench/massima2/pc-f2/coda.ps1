# Massima 2.0 - fase 2, punti pesanti sul PC. Ripartibile: i blocchi finiti restano in cache\.
Set-Location -Path $PSScriptRoot
$env:BRISCOLA_POLICY = "politica.bin"
$env:BRISCOLA_SIM_CACHE = "cache"
$massima = "infer:20000:1:1000:0.25"
New-Item -ItemType Directory -Force -Path ris | Out-Null
foreach ($riga in Get-Content punti.txt) {
  $nome, $spec = $riga -split '\|', 2
  if ((Test-Path "ris\$nome.txt") -and ((Get-Item "ris\$nome.txt").Length -gt 0)) { continue }
  & .\briscola-sim.exe --deals 5000 --player $spec --opponent $massima --seed 20261117 > "ris\$nome.tmp" 2>> log.txt
  if ($LASTEXITCODE -eq 0) { Move-Item -Force "ris\$nome.tmp" "ris\$nome.txt" }
}
"FINITO" | Out-File fine.txt
